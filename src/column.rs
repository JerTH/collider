use std::{
    cell::UnsafeCell,
    fmt::Display,
    ptr::NonNull, any::Any,
};

use crate::{
    borrowed::{BorrowError, BorrowRef, BorrowRefMut, BorrowSentinel, RawBorrow},
    database::{
        reckoning::{AnyPtr, DbError},
        Component, ComponentType,
    },
    id::{StableTypeId, CommutativeId, FamilyId},
    EntityId,
};

pub struct ColumnRef<C: Component> {
    borrow: BorrowRef,
    ptr_entity_map: NonNull<Vec<EntityId>>,
    ptr_components: NonNull<Vec<C>>,
}

impl<'b, C: Component> ColumnRef<C> {
    pub fn new(borrow: BorrowRef, ptr_components: NonNull<Vec<C>>, ptr_entity_map: NonNull<Vec<EntityId>>) -> Self {
        Self { borrow, ptr_components, ptr_entity_map }
    }

    pub fn ascend(self) -> ColumnRefMut<C> {
        ColumnRefMut {
            borrow: self.borrow.ascend(),
            ptr_entity_map: self.ptr_entity_map,
            ptr_components: self.ptr_components,
        }
    }
}

//impl<C: Component> Deref for ColumnRef<C> {
//    type Target = Vec<C>;
//    fn deref(&self) -> &Self::Target {
//        // SAFETY
//        // Safe to access because we hold a runtime checked borrow
//        unsafe { self.ptr_components.as_ref() }
//    }
//}

pub struct ColumnRefMut<C> {
    borrow: BorrowRefMut,
    ptr_entity_map: NonNull<Vec<EntityId>>,
    ptr_components: NonNull<Vec<C>>,
}

impl<'b, C: Component> From<ColumnRef<C>> for ColumnRefMut<C> {
    fn from(value: ColumnRef<C>) -> Self {
        value.ascend()
    }
}

impl<C: Component> ColumnRefMut<C> {
    pub fn new(borrow: BorrowRefMut, ptr_components: NonNull<Vec<C>>, ptr_entity_map: NonNull<Vec<EntityId>>) -> Self {
        Self { borrow, ptr_components, ptr_entity_map }
    }

    pub(crate) unsafe fn push_instance(&mut self, entity: EntityId) -> usize {
        let (components, entity_map) = self.deref_parts_mut();
        components.push(Default::default());
        entity_map.push(entity);
        entity_map.len() - 1
    }

    pub(crate) unsafe fn swap_and_destroy(&mut self, index: usize) -> ColumnSwapRemoveResult {
        let (components, entity_map) = self.deref_parts_mut();
        let _ = components.swap_remove(index);
        let removed_entity = entity_map.swap_remove(index);
        
        debug_assert_eq!(components.len(), entity_map.len());

        ColumnSwapRemoveResult {
            moved: *entity_map.get_unchecked(index),
            removed: removed_entity,
            new_moved_index: index,
            new_column_len: entity_map.len(),
        }
    }

    pub(crate) unsafe fn move_component_to(&mut self, dest: &mut Self, index: usize) -> Result<ColumnMoveResult, DbError> {
        let (from_components, from_entity_map) = self.deref_parts_mut();
        let (dest_components, dest_entity_map) = dest.deref_parts_mut();

        match from_components.len() {
            0 => {
                return Ok(ColumnMoveResult::NoMove)
            },
            1 => {
                let (removed, removed_id) = (from_components.pop(), from_entity_map.pop());
                let (removed, removed_id) = (removed.expect("expected component"), removed_id.expect("expected entity id"));

                dest_components.push(removed);
                dest_entity_map.push(removed_id.clone());

                let new_moved_index = dest_entity_map.len() - 1;
                let moved = *dest_entity_map.get_unchecked(new_moved_index);

                debug_assert_eq!(moved, removed_id);

                return Ok(ColumnMoveResult::Moved {
                    moved,
                    new_moved_index,
                })
            },
            _ => {
                let (removed, removed_id) = (from_components.swap_remove(index), from_entity_map.swap_remove(index));

                let swapped = *from_entity_map.get_unchecked(index);
                let new_swapped_index = index;

                dest_components.push(removed);
                dest_entity_map.push(removed_id.clone());

                let new_moved_index = dest_entity_map.len() - 1;
                let moved = *dest_entity_map.get_unchecked(new_moved_index);

                debug_assert_eq!(moved, removed_id);

                return Ok(ColumnMoveResult::SwapMoved {
                    moved,
                    new_moved_index,
                    swapped,
                    new_swapped_index,
                })
            },
        }
    }

    pub(crate) unsafe fn set_component(&mut self, entity: &EntityId, index: usize, component: C) -> Result<(), DbError> {
        let (components, entities) = self.deref_parts_mut();
        debug_assert_eq!(components.len(), entities.len());

        if components.len() == index {
            components.push(component);
            entities.push(*entity);
        } else {
            if components.len() > index {
                let component_ref = components.get_mut(index).ok_or(DbError::ColumnAccessOutOfBounds)?;
                *component_ref = component;
            } else {
                return Err(DbError::ColumnAccessOutOfBounds)
            }
        }
        
        Ok(())
    }

    unsafe fn deref_parts_mut(&mut self) -> (&mut Vec<C>, &mut Vec<EntityId>) {
        (self.ptr_components.as_mut(), self.ptr_entity_map.as_mut())
    }
}

//impl<C> Deref for ColumnRefMut<C> {
//    type Target = Vec<C>;
//    fn deref(&self) -> &Self::Target {
//        // SAFETY
//        // Safe to access because we hold a runtime checked borrow
//        unsafe { self.ptr_components.as_ref() }
//    }
//}
//
//impl<C> DerefMut for ColumnRefMut<C> {
//    fn deref_mut(&mut self) -> &mut Self::Target {
//        // SAFETY
//        // Safe to access because we hold a runtime checked borrow
//        unsafe { self.ptr_components.as_mut() }
//    }
//}

pub const COLUMN_LENGTH_MAXIMUM: usize = 2048; // 16384

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColumnKey(CommutativeId);

/// Combines a family ID and a type id into a column key
/// Each family can have exactly one column of a given type
impl From<(FamilyId, ComponentType)> for ColumnKey {
    fn from(value: (FamilyId, ComponentType)) -> Self {
        ColumnKey(CommutativeId::from((value.0, value.1)))
    }
}

/// A header describing a certain typed column. Stores the necessary functions
/// to construct a column of its own type and interact with it, and other
/// meta data associated with the column
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq)]
pub struct ColumnHeader {
    pub tyid: StableTypeId,
    pub(crate) fn_constructor: fn() -> AnyPtr,
    pub(crate) fn_instance: fn(&AnyPtr, &EntityId) -> usize,
    pub(crate) fn_move: fn(&AnyPtr, &AnyPtr, usize) -> Result<ColumnMoveResult, DbError>,
    pub(crate) fn_swap_and_destroy: fn(&AnyPtr, usize) -> ColumnSwapRemoveResult,
    pub(crate) fn_debug: fn(&AnyPtr, &mut std::fmt::Formatter<'_>) -> std::fmt::Result,
}

impl ColumnHeader {
    pub const fn stable_type_id(&self) -> StableTypeId {
        self.tyid
    }
}

/// A type erased container for storing a contiguous column of data
#[derive(Debug)]
pub struct Column {
    pub header: ColumnHeader, // Meta-data and function ptrs
    pub data: AnyPtr,       // ColumnInner<T>
}

/// Used to break apart a run-time tracked borrow into its component parts (an atomic borrow and a pointer)
pub trait BorrowAsRawParts {
    unsafe fn borrow_as_raw_parts(self) -> (RawBorrow, NonNull<std::os::raw::c_void>);
}

impl<C: Component> BorrowAsRawParts for ColumnRef<C> {
    unsafe fn borrow_as_raw_parts(self) -> (RawBorrow, NonNull<std::os::raw::c_void>) {
        let ptr = self.ptr_components.as_ptr() as *mut std::os::raw::c_void;
        let non_null = NonNull::new(ptr)
            .expect("expeceted non-null pointer");

        (RawBorrow::Immutable(self.borrow), non_null)
    }
}

impl<C: Component> BorrowAsRawParts for ColumnRefMut<C> {
    unsafe fn borrow_as_raw_parts(self) -> (RawBorrow, NonNull<std::os::raw::c_void>) {
        let ptr = self.ptr_components.as_ptr() as *mut std::os::raw::c_void;
        let non_null = NonNull::new(ptr)
            .expect("expeceted non-null pointer");
        
        (RawBorrow::Mutable(self.borrow), non_null)
    }
}

pub trait BorrowColumnAs<C, R> {
    fn borrow_column_as(&self) -> R;
}

impl<C: Component> BorrowColumnAs<C, ColumnRef<C>> for Column {
    fn borrow_column_as(&self) -> ColumnRef<C> {
        self.get_inner_ref()
    }
}

impl<C: Component> BorrowColumnAs<C, ColumnRefMut<C>> for Column {
    fn borrow_column_as(&self) -> ColumnRefMut<C> {
        self.get_inner_ref().ascend()
    }
}

impl<'b> Column {
    pub fn new(header: ColumnHeader, data: AnyPtr) -> Column {
        Column { header, data }
    }

    pub(crate) fn instance_with<C: Component>(&mut self, entity: &EntityId, component: C) -> Result<usize, DbError> {
        let index = self.instance(entity)?;
        unsafe { self.get_inner_ref_mut().set_component(entity, index, component)? }
        Ok(index)
    }

    /// Instances an entity with a given [EntityId] in the [Column] and returns its columnar index on success
    pub(crate) fn instance(&mut self, entity: &EntityId) -> Result<usize, DbError> {
        Ok((self.header.fn_instance)(&self.data, entity))
    }

    pub(crate) fn get_inner_ref<C: Component>(&'b self) -> ColumnRef<C> {
        ColumnInner::<C>::downcast_and_borrow(&self.data).expect("expected column access")
    }

    pub(crate) fn get_inner_ref_mut<C: Component>(&'b self) -> ColumnRefMut<C> {
        ColumnInner::<C>::downcast_and_borrow_mut(&self.data).expect("expected column access")
    }
    
    /// Destroys an entity by swapping it to the end of the column and then popping it
    /// Returns the [EntityId] of the *swapped* entity, and the new length of this column if successful
    pub(crate) fn swap_and_destroy(&mut self, index: usize) -> ColumnSwapRemoveResult {
        (self.header.fn_swap_and_destroy)(&self.data, index)
    }

    /// Moves an entity from the given index of this [Column] to the end of `dest` [Column]
    /// Returns the [EntityId] of the *swapped* entity, and the new length of the `dest` column if successful
    pub(crate) fn move_component_to(&mut self, dest: &mut Self, index: usize) -> Result<ColumnMoveResult, DbError> {
        (self.header.fn_move)(&self.data, &dest.data, index)
    }

    pub(crate) fn set_component<C: Component>(&mut self, entity: &EntityId, index: usize, component: C) -> Result<(), DbError> {
        let mut column_ref = ColumnInner::<C>::downcast_and_borrow_mut(&self.data).expect("expected column access");
        unsafe { column_ref.set_component(entity, index, component) }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub(crate) enum ColumnMoveResult {
    Moved {
        moved: EntityId,
        new_moved_index: usize,
    },
    SwapMoved {
        moved: EntityId,
        new_moved_index: usize,
        swapped: EntityId,
        new_swapped_index: usize,
    },
    NoMove,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ColumnSwapRemoveResult {
    pub(crate) moved: EntityId,
    pub(crate) removed: EntityId,
    pub(crate) new_moved_index: usize,
    pub(crate) new_column_len: usize,
}

/// The actual raw data storage for the users data
#[derive(Debug, Default)]
pub(crate) struct ColumnInner<C: Component> {
    /// INVARIANT:
    ///
    /// For an entity in a table, its associated components must
    /// always occupy the same index in each column. Failure to
    /// uphold this invariant will result in undefined behavior
    pub(crate) borrow: BorrowSentinel,
    pub(crate) values: UnsafeCell<Vec<C>>,
    pub(crate) entity: UnsafeCell<Vec<EntityId>>,
}

impl<C: Component> ColumnInner<C> {
    fn new() -> Self {
        Default::default()
    }
}

impl<C: Component> Display for ColumnInner<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\nColumnData ({:?})\n{{\n", self.borrow)?;

        let (values, entities) = unsafe { (&*self.values.get(), &*self.entity.get()) };

        for (c, e) in values.iter().zip(entities.iter()) {
            write!(f, "\t{:?} --> {:?}\n", c, e)?;
        } 
        write!(f, "}}\n")
    }
}

impl<'b, C: Component> ColumnInner<C> {
    pub fn borrow_column(&'b self) -> ColumnRef<C> {
        self.try_borrow()
            .expect("column was already mutably borrowed")
    }

    fn try_borrow(&'b self) -> Result<ColumnRef<C>, BorrowError> {
        match BorrowRef::new(self.borrow.clone()) {
            Some(borrow) => {
                let ptr_components = unsafe { NonNull::new_unchecked(self.values.get()) };
                let ptr_entity_map = unsafe { NonNull::new_unchecked(self.entity.get()) };
                Ok(ColumnRef::new(borrow, ptr_components, ptr_entity_map))
            }
            None => Err(BorrowError::AlreadyBorrowed),
        }
    }

    pub fn borrow_column_mut(&'b self) -> ColumnRefMut<C> {
        self.try_borrow_mut().expect("column was already borrowed")
    }

    fn try_borrow_mut(&'b self) -> Result<ColumnRefMut<C>, BorrowError> {
        match BorrowRefMut::new(self.borrow.clone()) {
            Some(borrow) => {
                let ptr_components = unsafe { NonNull::new_unchecked(self.values.get()) };
                let ptr_entity_map = unsafe { NonNull::new_unchecked(self.entity.get()) };
                Ok(ColumnRefMut::new(borrow, ptr_components, ptr_entity_map))
            }
            None => Err(BorrowError::AlreadyBorrowed),
        }
    }

    fn downcast_column(column: &AnyPtr) -> &ColumnInner<C> {
        column.downcast_ref::<ColumnInner<C>>().expect("column type mismatch when attempting to downcast")
    }
    
    pub fn dynamic_debug(column: &AnyPtr, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result{
        let column: &ColumnInner<C> = ColumnInner::downcast_column(column);
        (column as &dyn Display).fmt(f)
    }

    fn is_same_column(from_ptr: &AnyPtr, dest_ptr: &AnyPtr) -> bool {
        let raw_ptr_from: *const dyn Any = from_ptr.as_ref();
        let raw_ptr_dest: *const dyn Any = dest_ptr.as_ref();
        raw_ptr_from == raw_ptr_dest
    }

    fn downcast_and_borrow(column: &AnyPtr) -> Result<ColumnRef<C>, DbError> {
        Ok(column
            .downcast_ref::<ColumnInner<C>>()
            .ok_or(DbError::ColumnTypeDiscrepancy)?
            .borrow_column())
    }

    fn downcast_and_borrow_mut(column: &AnyPtr) -> Result<ColumnRefMut<C>, DbError> {
        Ok(column
            .downcast_ref::<ColumnInner<C>>()
            .ok_or(DbError::ColumnTypeDiscrepancy)?
            .borrow_column_mut())
    }

    /// Moves a single component from one [Column] to another, if they are the same type,
    /// leaving an empty space in the source column
    //pub fn dynamic_move(
    //    from_ptr: &AnyPtr,
    //    dest_ptr: &AnyPtr,
    //    from_index: usize,
    //    dest_index: usize,
    //) -> Result<(), DbError> {
    //    if ColumnInner::<C>::is_same_column(from_ptr, dest_ptr) { return Ok(()) }
    //    
    //    let mut from = ColumnInner::<C>::downcast_and_borrow_mut(from_ptr)?;
    //    let mut dest = ColumnInner::<C>::downcast_and_borrow_mut(dest_ptr)?;
//
    //    debug_assert!(from.len() > from_index);
    //    debug_assert!(dest.len() > dest_index);
//
    //    dest[dest_index] = from[from_index].clone();
    //    from[from_index] = Default::default();
//
    //    Ok(())
    //}
    
    /// Moves a single component from the given index in `from` [Column] to the end of
    /// `dest` column. Maintains compaction of both columns by swapping the last
    /// element of the source column into the newly free space
    /// 
    /// Returns the new length of both columns
    pub fn dynamic_move(
        from_ptr: &AnyPtr,
        dest_ptr: &AnyPtr,
        from_index: usize,
    ) -> Result<ColumnMoveResult, DbError> {
        if ColumnInner::<C>::is_same_column(from_ptr, dest_ptr) { return Err(DbError::MoveWithSameColumn) }

        let mut from = ColumnInner::<C>::downcast_and_borrow_mut(from_ptr)?;
        let mut dest = ColumnInner::<C>::downcast_and_borrow_mut(dest_ptr)?;

        unsafe { from.move_component_to(&mut dest, from_index) }
    }
    
    /// Resizes the column to hold at least [min_size] components
    //pub fn dynamic_resize(column: &AnyPtr, min_size: usize) -> Result<usize, DbError> {
    //    let inner: &ColumnInner<C> = ColumnInner::downcast_column(column);
    //    inner.resize_minimum(min_size)
    //}
    
    pub fn dynamic_swap_and_destroy(column: &AnyPtr, index: usize) -> ColumnSwapRemoveResult {
        let mut column_ref: ColumnRefMut<C> = ColumnInner::downcast_column(column).borrow_column_mut();
        unsafe { column_ref.swap_and_destroy(index) }
    }

    /// Constructs a [Column] and returns a type erased pointer to it
    pub fn dynamic_ctor() -> AnyPtr {
        Box::new(ColumnInner::<C>::new())
    }
    
    /// Creates a new instance of the component type C and returns the index of the instance
    pub fn dynamic_push_instance(column: &AnyPtr, entity: &EntityId) -> usize {
        let mut col_ref_mut = ColumnInner::<C>::downcast_column(column).borrow_column_mut();
        unsafe { col_ref_mut.push_instance(*entity) }
    }

    //pub fn resize_minimum(&self, min_size: usize) -> Result<usize, DbError> {
    //    let power_of_two_index = std::cmp::max(min_size, 1).next_power_of_two();
    //    debug_assert!(power_of_two_index < COLUMN_LENGTH_MAXIMUM);
//
    //    let mut column = self.borrow_column_mut();
    //    let len = column.len();
    //    
    //    if min_size >= len {
    //        column.resize_with(power_of_two_index, Default::default);
    //    }
    //    
    //    debug_assert!(column.len() >= min_size);
//
    //    Ok(column.len())
    //}
}


// Display impl

impl Display for Column {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Column\n[\n")?;
        write!(f, "{:#?}", self.header)?;
        (self.header.fn_debug)(&self.data, f)?;
        write!(f, "]\n")
    }
}
