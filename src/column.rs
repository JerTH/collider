use std::{
    cell::UnsafeCell,
    fmt::Display,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull, any::Any, borrow::Borrow, os::raw::c_void,
};

use crate::{
    borrowed::{BorrowError, BorrowRef, BorrowRefMut, BorrowSentinel, BorrowRefEither},
    database::{
        reckoning::{AnyPtr, DbError, Family},
        Component, ComponentType,
    },
    id::{StableTypeId, CommutativeId, FamilyId},
    EntityId, transform::{Read, Write},
};

pub struct ColumnRef<'b, C: Component> {
    pointer: NonNull<Vec<C>>,
    borrow: BorrowRef<'b>,
}

impl<'b, C: Component> ColumnRef<'b, C> {
    pub fn new(column: NonNull<Vec<C>>, borrow: BorrowRef<'b>) -> Self {
        Self { pointer: column, borrow }
    }

    pub fn ascend(self) -> ColumnRefMut<'b, C> {
        ColumnRefMut {
            pointer: self.pointer,
            borrow: self.borrow.ascend(),
        }
    }
}

impl<C: Component> Deref for ColumnRef<'_, C> {
    type Target = Vec<C>;
    fn deref(&self) -> &Self::Target {
        // SAFETY
        // Safe to access because we hold a runtime checked borrow
        unsafe { self.pointer.as_ref() }
    }
}

impl<'b, C: Component> IntoIterator for ColumnRef<'b, C> {
    type Item = &'b C;
    type IntoIter = ColumnIter<'b, C>;
    fn into_iter(self) -> Self::IntoIter {
        let size = self.len();
        ColumnIter {
            column: unsafe { self.pointer.as_ref() },
            borrow: self.borrow,
            size,
            next: 0usize,
        }
    }
}

pub struct ColumnIter<'b, C> {
    column: &'b Vec<C>,
    borrow: BorrowRef<'b>,
    size: usize,
    next: usize,
}

impl<'b, C: 'b> Iterator for ColumnIter<'b, C> {
    type Item = &'b C;

    fn next(&mut self) -> Option<Self::Item> {
        let val = self.column.get(self.next);
        self.next += 1;
        val
    }
}

pub struct ColumnRefMut<'b, C> {
    pointer: NonNull<Vec<C>>,
    borrow: BorrowRefMut<'b>,
}

impl<'b, C: Component> From<ColumnRef<'b, C>> for ColumnRefMut<'b, C> {
    fn from(value: ColumnRef<'b, C>) -> Self {
        value.ascend()
    }
}

impl<'b, T> ColumnRefMut<'b, T> {
    pub fn new(column: NonNull<Vec<T>>, borrow: BorrowRefMut<'b>) -> Self {
        Self { pointer: column, borrow }
    }
}

impl<C> Deref for ColumnRefMut<'_, C> {
    type Target = Vec<C>;
    fn deref(&self) -> &Self::Target {
        // SAFETY
        // Safe to access because we hold a runtime checked borrow
        unsafe { self.pointer.as_ref() }
    }
}

impl<C> DerefMut for ColumnRefMut<'_, C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY
        // Safe to access because we hold a runtime checked borrow
        unsafe { self.pointer.as_mut() }
    }
}

impl<'b, C: 'b> IntoIterator for ColumnRefMut<'b, C> {
    type Item = &'b mut C;
    type IntoIter = ColumnIterMut<'b, C>;
    fn into_iter(mut self) -> Self::IntoIter {
        let size = self.len();
        Self::IntoIter {
            column: unsafe { self.pointer.as_mut() },
            borrow: self.borrow,
            size,
            next: 0usize,
            invariant: PhantomData::default(),
        }
    }
}

pub struct ColumnIterMut<'b, C> {
    column: &'b mut Vec<C>,
    borrow: BorrowRefMut<'b>,
    size: usize,
    next: usize,
    invariant: PhantomData<&'b mut C>,
}
impl<'b, T: 'b> Iterator for ColumnIterMut<'b, T> {
    type Item = &'b mut T;
    fn next<'n>(&'n mut self) -> Option<Self::Item> {
        let val = self.column.get_mut(self.next);
        self.next = std::cmp::min(self.next + 1, self.size);
        // SAFETY
        // This is safe because we offer no other interface for accessing the items
        // we are iterating over, and we promise to only ever yield one mutable
        // reference to any given element, while upholding runtime borrow checks
        unsafe { std::mem::transmute::<Option<&mut T>, Option<&'b mut T>>(val) }
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
    pub fn_move: fn(&AnyPtr, &AnyPtr, usize, usize) -> Result<(), DbError>,
    pub fn_resize: fn(&AnyPtr, usize) -> Result<usize, DbError>,
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

pub trait BorrowAsRawParts<'b> {
    unsafe fn borrow_as_raw_parts(self) -> (BorrowRefEither<'b>, NonNull<std::os::raw::c_void>);
}

impl<'b, C: Component> BorrowAsRawParts<'b> for ColumnRef<'b, C> {
    unsafe fn borrow_as_raw_parts(self) -> (BorrowRefEither<'b>, NonNull<std::os::raw::c_void>) {
        let ptr = self.pointer.as_ptr() as *mut std::os::raw::c_void;
        let non_null = NonNull::new(ptr)
            .expect("expeceted non-null pointer");

        (BorrowRefEither::Immutable(self.borrow), non_null)
    }
}

impl<'b, C: Component> BorrowAsRawParts<'b> for ColumnRefMut<'b, C> {
    unsafe fn borrow_as_raw_parts(self) -> (BorrowRefEither<'b>, NonNull<std::os::raw::c_void>) {
        let ptr = self.pointer.as_ptr() as *mut std::os::raw::c_void;
        let non_null = NonNull::new(ptr)
            .expect("expeceted non-null pointer");
        
        (BorrowRefEither::Mutable(self.borrow), non_null)
    }
}

pub trait BorrowColumnAs<'b, C, R> {
    fn borrow_column_as(&'b self) -> R;
}

impl<'b, C: Component> BorrowColumnAs<'b, C, ColumnRef<'b, C>> for Column {
    fn borrow_column_as(&'b self) -> ColumnRef<'b, C> {
        self.get_ref()
    }
    //fn borrow_column_as<ColumnRef<'b, C>>(&'b self) -> ColumnRef<'b, C> {
    //    self.get_ref()
    //}
}

impl<'b, C: Component> BorrowColumnAs<'b, C, ColumnRefMut<'b, C>> for Column {
    fn borrow_column_as(&'b self) -> ColumnRefMut<'b, C> {
        self.get_ref().ascend()
    }
    //fn borrow_column_as(&'b self) -> ColumnRefMut<'b, C> {
    //    let r = self.get_ref();
    //    r.ascend()
    //}
}

impl<'b> Column {
    pub fn new(header: ColumnHeader, data: AnyPtr) -> Column {
        Column { header, data }
    }

    /// Instantiate a component instance at the specified index
    /// This function doesn't actually really care about the data
    /// stored at a given index, it just cares that the memory at
    /// the provided index is initialized
    pub fn instantiate_at(&'b self, index: usize) -> Result<(), DbError> {
        let minimum_size = index + 1;
        (self.header.fn_resize)(&self.data, minimum_size).map(|_| ())
    }

    pub fn instantiate_with<C: Component>(&'b self, index: usize, component: C) -> Result<(), DbError> {
        self.instantiate_at(index)?;

        let column = self.data
            .downcast_ref::<ColumnInner<C>>()
            .expect("instantiate_with: expected matching column types");
        let mut column_ref = column.borrow_column_mut();
        column_ref[index] = component;
        Ok(())
    }
    
    pub(crate) fn iter<T: Component>(&'b self) -> ColumnIter<'b, T> {
        debug_assert!(StableTypeId::of::<T>() == self.header.tyid);
        let column = self
            .data
            .downcast_ref::<ColumnInner<T>>()
            .expect("iter: expected matching column types");
        let column_ref = column.borrow_column();
        let column_iter = column_ref.into_iter();
        column_iter
    }

    pub(crate) fn iter_mut<T: Component>(&'b self) -> ColumnIterMut<'b, T> {
        debug_assert!(StableTypeId::of::<T>() == self.header.tyid);
        let column = self
            .data
            .downcast_ref::<ColumnInner<T>>()
            .expect("iter_mut: expected matching column types");
        let column_mut = column.borrow_column_mut();
        let column_iter_mut = column_mut.into_iter();

        column_iter_mut
    }

    pub(crate) fn get_ref<C: Component>(&'b self) -> ColumnRef<'b, C> {
        let inner = self
            .data
            .downcast_ref::<ColumnInner<C>>()
            .expect("get_ref: expected matching column types");
        
        inner.borrow_column()
    }
}

impl Display for Column {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Column\n[\n")?;
        write!(f, "{:#?}", self.header)?;
        (self.header.fn_debug)(&self.data, f)?;
        write!(f, "]\n")
    }
}

/// The actual raw data storage for the users data
#[derive(Debug, Default)]
pub(crate) struct ColumnInner<C: Component> {
    /// INVARIANT:
    ///
    /// For an entity in a table, its associated components must
    /// always occupy the same index in each column. Failure to
    /// uphold this invariant will result in undefined behavior
    pub(crate) values: UnsafeCell<Vec<C>>,
    pub(crate) borrow: BorrowSentinel,
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
    pub fn borrow_column(&'b self) -> ColumnRef<'b, C> {
        self.try_borrow()
            .expect("column was already mutably borrowed")
    }

    fn try_borrow(&'b self) -> Result<ColumnRef<'b, C>, BorrowError> {
        match BorrowRef::new(&self.borrow) {
            Some(borrow) => {
                let column = unsafe { NonNull::new_unchecked(self.values.get()) };
                Ok(ColumnRef::new(column, borrow))
            }
            None => Err(BorrowError::AlreadyBorrowed),
        }
    }

    pub fn borrow_column_mut(&'b self) -> ColumnRefMut<'b, C> {
        self.try_borrow_mut().expect("column was already borrowed")
    }

    fn try_borrow_mut(&'b self) -> Result<ColumnRefMut<'b, C>, BorrowError> {
        match BorrowRefMut::new(&self.borrow) {
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

    /// Moves a single component from one [Column] to another, if they are the same type
    pub fn dynamic_move(
        from_ptr: &AnyPtr,
        dest_ptr: &AnyPtr,
        from_index: usize,
        dest_index: usize,
    ) -> Result<(), DbError> {

        println!("DYNAMIC MOVE");
        { // guard scope
            let raw_ptr_from: *const dyn Any = from_ptr.as_ref();
            let raw_ptr_dest: *const dyn Any = dest_ptr.as_ref();
            if raw_ptr_from == raw_ptr_dest {
                println!("\tNO MOVE NECESSARY...{:?}=={:?}", raw_ptr_from, raw_ptr_dest);
                return Ok(()) // no move necessary - same objects
            }
        }
        
        let mut from = from_ptr
            .downcast_ref::<ColumnInner<C>>()
            .ok_or(DbError::ColumnTypeDiscrepancy)?
            .borrow_column_mut();

        let mut dest = dest_ptr
            .downcast_ref::<ColumnInner<C>>()
            .ok_or(DbError::ColumnTypeDiscrepancy)?
            .borrow_column_mut();

        println!("\tDYNAMIC MOVE");
        println!("\tLENS: {}, {}", from.len(), dest.len());

        debug_assert!(from.len() > from_index);
        debug_assert!(dest.len() > dest_index);

        dest[dest_index] = from[from_index].clone();
        //dest.insert(dest_index, from[from_index].clone());

        println!("SETTING SOURCE TO DEFAULT");
        from[from_index] = Default::default();

        Ok(())
    }

    /// Resizes the column to hold at least [min_size] components
    pub fn dynamic_resize(column: &AnyPtr, min_size: usize) -> Result<usize, DbError> {
        let column: &ColumnInner<C> = ColumnInner::downcast_column(column);
        column.resize_minimum(min_size)
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
