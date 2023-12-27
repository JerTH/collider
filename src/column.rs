use std::{
    cell::UnsafeCell,
    fmt::Display,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull, any::{Any, TypeId, type_name}, borrow::Borrow, os::raw::c_void,
};

use crate::{
    borrowed::{BorrowError, BorrowRef, BorrowRefMut, BorrowSentinel, RawBorrow},
    database::{
        reckoning::{AnyPtr, DbError, Family},
        Component, ComponentType,
    },
    id::{StableTypeId, CommutativeId, FamilyId},
    EntityId, transform::{Read, Write},
};

pub struct ColumnRef<C: Component> {
    pointer: NonNull<Vec<C>>,
    borrow: BorrowRef,
}

impl<'b, C: Component> ColumnRef<C> {
    pub fn new(column: NonNull<Vec<C>>, borrow: BorrowRef) -> Self {
        Self { pointer: column, borrow }
    }

    pub fn ascend(self) -> ColumnRefMut<C> {
        ColumnRefMut {
            pointer: self.pointer,
            borrow: self.borrow.ascend(),
        }
    }
}

impl<C: Component> Deref for ColumnRef<C> {
    type Target = Vec<C>;
    fn deref(&self) -> &Self::Target {
        // SAFETY
        // Safe to access because we hold a runtime checked borrow
        unsafe { self.pointer.as_ref() }
    }
}

pub struct ColumnRefMut<C> {
    pointer: NonNull<Vec<C>>,
    borrow: BorrowRefMut,
}

impl<'b, C: Component> From<ColumnRef<C>> for ColumnRefMut<C> {
    fn from(value: ColumnRef<C>) -> Self {
        value.ascend()
    }
}

impl<T> ColumnRefMut<T> {
    pub fn new(column: NonNull<Vec<T>>, borrow: BorrowRefMut) -> Self {
        Self { pointer: column, borrow }
    }
}

impl<C> Deref for ColumnRefMut<C> {
    type Target = Vec<C>;
    fn deref(&self) -> &Self::Target {
        // SAFETY
        // Safe to access because we hold a runtime checked borrow
        unsafe { self.pointer.as_ref() }
    }
}

impl<C> DerefMut for ColumnRefMut<C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY
        // Safe to access because we hold a runtime checked borrow
        unsafe { self.pointer.as_mut() }
    }
}

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
    pub fn_constructor: fn() -> AnyPtr,
    pub fn_instance: fn(&AnyPtr, usize),
    pub fn_move: fn(&AnyPtr, &AnyPtr, usize) -> Result<(usize, usize), DbError>,
    pub fn_resize: fn(&AnyPtr, usize) -> Result<usize, DbError>,
    pub fn_swap_and_destroy: fn(&AnyPtr, usize) -> Option<usize>,
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
    pub entitym: Vec<EntityId>,
}

/// Used to break apart a run-time tracked borrow into its component parts (an atomic borrow and a pointer)
pub trait BorrowAsRawParts {
    unsafe fn borrow_as_raw_parts(self) -> (RawBorrow, NonNull<std::os::raw::c_void>);
}

impl<C: Component> BorrowAsRawParts for ColumnRef<C> {
    unsafe fn borrow_as_raw_parts(self) -> (RawBorrow, NonNull<std::os::raw::c_void>) {
        let ptr = self.pointer.as_ptr() as *mut std::os::raw::c_void;
        let non_null = NonNull::new(ptr)
            .expect("expeceted non-null pointer");

        (RawBorrow::Immutable(self.borrow), non_null)
    }
}

impl<C: Component> BorrowAsRawParts for ColumnRefMut<C> {
    unsafe fn borrow_as_raw_parts(self) -> (RawBorrow, NonNull<std::os::raw::c_void>) {
        let ptr = self.pointer.as_ptr() as *mut std::os::raw::c_void;
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
        //println!("BorrowColumnAs -> ColumnType:    {:?} / {:?}", TypeId::of::<ColumnInner<C>>(), type_name::<C>());
        self.get_ref()
    }
    //fn borrow_column_as<ColumnRef<'b, C>>(&'b self) -> ColumnRef<'b, C> {
    //    self.get_ref()
    //}
}

impl<C: Component> BorrowColumnAs<C, ColumnRefMut<C>> for Column {
    fn borrow_column_as(&self) -> ColumnRefMut<C> {
        //println!("BorrowColumnAsMut -> ColumnType: {:?} / {:?}", TypeId::of::<ColumnInner<C>>(), type_name::<C>());
        self.get_ref().ascend()
    }
    //fn borrow_column_as(&'b self) -> ColumnRefMut<'b, C> {
    //    let r = self.get_ref();
    //    r.ascend()
    //}
}

impl<'b> Column {
    pub fn new(header: ColumnHeader, data: AnyPtr) -> Column {
        Column { header, data, entitym: Default::default() }
    }

    fn len(&self) -> usize {
        self.entitym.len()
    }

    /// Instantiate a component instance at the specified index
    /// This function doesn't actually really care about the data
    /// stored at a given index, it just cares that the memory at
    /// the provided index is initialized
    pub(crate) fn instantiate_at(&self, index: usize) -> Result<(), DbError> {
        let minimum_size = index + 1;
        (self.header.fn_resize)(&self.data, minimum_size).map(|_| ())
    }

    pub(crate) fn instantiate_with<C: Component>(&self, index: usize, component: C) -> Result<(), DbError> {
        self.instantiate_at(index)?;
        self.get_mut()[index] = component;

        Ok(())
    }
    
    pub(crate) fn get_ref<C: Component>(&'b self) -> ColumnRef<C> {
        ColumnInner::<C>::downcast_and_borrow(&self.data).expect("expected matching column types")
    }

    pub(crate) fn get_mut<C: Component>(&'b self) -> ColumnRefMut<C> {
        ColumnInner::<C>::downcast_and_borrow_mut(&self.data).expect("expected matching column types")
    }
    
    /// Destroys an entity by swapping it to the end of the column and then popping it
    /// Returns the [EntityId] of the *swapped* entity, and the new length of this column if successful
    pub(crate) fn swap_and_destroy(&mut self, index: usize) -> Option<(EntityId, usize)> {
        if let Some(new_len) = (self.header.fn_swap_and_destroy)(&self.data, index) {
            let _killed = self.entitym.swap_remove(index);
            let swapped = unsafe { *self.entitym.get_unchecked(index) };
            return Some((swapped, new_len));
        } else {
            return None;
        }
    }

    /// Moves an entity from the given index of this [Column] to the end of `dest` [Column]
    /// Returns the [EntityId] of the *swapped* entity, and the new length of the `dest` column if successful
    pub(crate) fn move_component_to(&mut self, dest: &mut Self, index: usize) -> Option<MoveComponentResult> {
        if let Ok((from_len, dest_len)) = (self.header.fn_move)(&self.data, &dest.data, index) {
            //dest.push(from.swap_remove(from_index));
            dest.entitym.push(self.entitym.swap_remove(index));

            let swapped = unsafe { *self.entitym.get_unchecked(index) };
            let moved = *dest.entitym.last().expect("expected just moved component");

            return Some(MoveComponentResult {
                moved,
                swapped,
                new_from_len: self.len(),
                new_dest_len: dest.len(),
            });

        } else {
            return None;
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub(crate) struct MoveComponentResult {
    pub(crate) moved: EntityId,
    pub(crate) swapped: EntityId,
    pub(crate) new_from_len: usize,
    pub(crate) new_dest_len: usize,
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
}

impl<C: Component> ColumnInner<C> {
    fn new() -> Self {
        Default::default()
    }
}

impl<C: Component> Display for ColumnInner<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\nColumnData ({:?})\n{{\n", self.borrow)?;
        for item in unsafe { &*self.values.get() } {
            write!(f, "\t{:?}\n", item)?;
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
                let column = unsafe { NonNull::new_unchecked(self.values.get()) };
                Ok(ColumnRef::new(column, borrow))
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
                let column = unsafe { NonNull::new_unchecked(self.values.get()) };
                Ok(ColumnRefMut::new(column, borrow))
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
    pub fn dynamic_pop_and_move(
        from_ptr: &AnyPtr,
        dest_ptr: &AnyPtr,
        from_index: usize,
    ) -> Result<(usize, usize), DbError> {
        if ColumnInner::<C>::is_same_column(from_ptr, dest_ptr) { return Err(DbError::MoveWithSameColumn) }
        
        let mut from = ColumnInner::<C>::downcast_and_borrow_mut(from_ptr)?;
        let mut dest = ColumnInner::<C>::downcast_and_borrow_mut(dest_ptr)?;
        
        debug_assert!(from.len() > from_index);
        dest.push(from.swap_remove(from_index));

        Ok((from.len(), dest.len()))
    }
    
    /// Resizes the column to hold at least [min_size] components
    pub fn dynamic_resize(column: &AnyPtr, min_size: usize) -> Result<usize, DbError> {
        let inner: &ColumnInner<C> = ColumnInner::downcast_column(column);
        inner.resize_minimum(min_size)
    }

    pub fn dynamic_swap_and_destroy(column: &AnyPtr, index: usize) -> Option<usize> {
        let mut column_ref: ColumnRefMut<C> = ColumnInner::downcast_column(column).borrow_column_mut();
        
        if column_ref.len() < index {
            return None;
        }

        let _ = column_ref.swap_remove(index);
        Some(column_ref.len())
    }

    /// Constructs a [Column] and returns a type erased pointer to it
    pub fn dynamic_ctor() -> AnyPtr {
        Box::new(ColumnInner::<C>::new())
    }

    /// Creates a new instance of the component type C at the specified index in the column
    pub fn dynamic_instance(column: &AnyPtr, index: usize) {
        unimplemented!()
    }

    pub fn resize_minimum(&self, min_size: usize) -> Result<usize, DbError> {
        let power_of_two_index = std::cmp::max(min_size, 1).next_power_of_two();
        debug_assert!(power_of_two_index < COLUMN_LENGTH_MAXIMUM);

        let mut column = self.borrow_column_mut();
        let len = column.len();
        
        if min_size >= len {
            column.resize_with(power_of_two_index, Default::default);
        }
        
        debug_assert!(column.len() >= min_size);

        Ok(column.len())
    }
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
