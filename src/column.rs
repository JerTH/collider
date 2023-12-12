use std::{
    cell::UnsafeCell,
    fmt::Display,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::{
    borrowed::{BorrowError, BorrowRef, BorrowRefMut, BorrowSentinel},
    database::{
        reckoning::{AnyPtr, DbError},
        Component,
    },
    id::StableTypeId,
    EntityId,
};

pub type ColumnType<C> = Vec<C>;

pub struct ColumnRef<'b, C: Component> {
    column: NonNull<ColumnType<C>>,
    borrow: BorrowRef<'b>,
}

impl<'b, C: Component> ColumnRef<'b, C> {
    pub fn new(column: NonNull<ColumnType<C>>, borrow: BorrowRef<'b>) -> Self {
        Self { column, borrow }
    }
}

impl<C: Component> Deref for ColumnRef<'_, C> {
    type Target = ColumnType<C>;
    fn deref(&self) -> &Self::Target {
        // SAFETY
        // Safe to access because we hold a runtime checked borrow
        unsafe { self.column.as_ref() }
    }
}

impl<'b, C: Component> IntoIterator for ColumnRef<'b, C> {
    type Item = &'b C;
    type IntoIter = ColumnIter<'b, C>;
    fn into_iter(self) -> Self::IntoIter {
        let size = self.len();
        ColumnIter {
            column: unsafe { self.column.as_ref() },
            borrow: self.borrow,
            size,
            next: 0usize,
        }
    }
}

pub struct ColumnIter<'b, C> {
    column: &'b ColumnType<C>,
    borrow: BorrowRef<'b>,
    size: usize,
    next: usize,
}

impl<'b, C: 'b> Iterator for ColumnIter<'b, C> {
    type Item = &'b C;

    fn next(&mut self) -> Option<Self::Item> {
        let val = self.column.get(self.next);
        self.next = std::cmp::min(self.next + 1, self.size);
        val
    }
}

pub struct ColumnRefMut<'b, T> {
    column: NonNull<Vec<T>>,
    borrow: BorrowRefMut<'b>,
}

impl<'b, T> ColumnRefMut<'b, T> {
    pub fn new(column: NonNull<Vec<T>>, borrow: BorrowRefMut<'b>) -> Self {
        Self { column, borrow }
    }
}

impl<T> Deref for ColumnRefMut<'_, T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        // SAFETY
        // Safe to access because we hold a runtime checked borrow
        unsafe { self.column.as_ref() }
    }
}

impl<T> DerefMut for ColumnRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY
        // Safe to access because we hold a runtime checked borrow
        unsafe { self.column.as_mut() }
    }
}

impl<'b, T: 'b> IntoIterator for ColumnRefMut<'b, T> {
    type Item = &'b mut T;
    type IntoIter = ColumnIterMut<'b, T>;
    fn into_iter(mut self) -> Self::IntoIter {
        let size = self.len();
        Self::IntoIter {
            column: unsafe { self.column.as_mut() },
            borrow: self.borrow,
            size,
            next: 0usize,
            invariant: PhantomData::default(),
        }
    }
}

pub struct ColumnIterMut<'b, T> {
    column: &'b mut Vec<T>,
    borrow: BorrowRefMut<'b>,
    size: usize,
    next: usize,
    invariant: PhantomData<&'b mut T>,
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

pub const COLUMN_LENGTH_MAXIMUM: usize = 2 ^ 14;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColumnKey(u64);

/// A type erased container for storing a contiguous column of data
#[derive(Debug)]
pub struct Column {
    pub header: ColumnHeader, // Meta-data and function ptrs
    pub data: AnyPtr,       // ColumnInner<T>
}

/// A header describing a certain typed column. Stores the necessary functions
/// to construct a column of its own type and interact with it, and other
/// meta data associated with the column
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq)]
pub struct ColumnHeader {
    tyid: StableTypeId,
    pub fn_constructor: fn() -> AnyPtr,
    pub fn_instance: fn(&AnyPtr, usize),
    pub fn_move: fn(&EntityId, usize, &AnyPtr, &AnyPtr) -> Result<usize, DbError>,
    pub fn_resize: fn(&AnyPtr, usize) -> usize,
}

impl<'b> Column {
    pub fn iter<T: Component>(&'b self) -> ColumnIter<'b, T> {
        debug_assert!(StableTypeId::of::<T>() == self.header.tyid);
        let column = self
            .data
            .downcast_ref::<ColumnInner<T>>()
            .expect("expected matching column types");
        let column_ref = column.borrow_column();
        let column_iter = column_ref.into_iter();
        column_iter
    }

    pub fn iter_mut<T: Component>(&'b self) -> ColumnIterMut<'b, T> {
        debug_assert!(StableTypeId::of::<T>() == self.header.tyid);
        let column = self
            .data
            .downcast_ref::<ColumnInner<T>>()
            .expect("expected matching column types");
        let column_mut = column.borrow_column_mut();
        let column_iter_mut = column_mut.into_iter();

        column_iter_mut
    }
}

/// The actual raw data storage for the users data
#[derive(Debug, Default)]
struct ColumnInner<C: Component> {
    /// INVARIANT:
    ///
    /// For an entity in a table, its associated components must
    /// always occupy the same index in each column. Failure to
    /// uphold this invariant will result in undefined behavior
    values: UnsafeCell<Vec<C>>,
    borrow: BorrowSentinel,
}

impl<C: Component> ColumnInner<C> {
    fn new() -> Self {
        Default::default()
    }
}

impl<C: Component> Display for ColumnInner<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Column\n")?;
        write!(f, "\t{:?}\n", self.borrow)?;
        write!(f, "\t{:?}\n", unsafe { &*self.values.get() })
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

    /// Moves a single component from one [Column] to another, if they are the same type
    pub fn dynamic_move(
        entity: &EntityId,
        index: usize,
        from: &AnyPtr,
        dest: &AnyPtr,
    ) -> Result<usize, DbError> {
        let mut from = from
            .downcast_ref::<ColumnInner<C>>()
            .ok_or(DbError::ColumnTypeDiscrepancy)?
            .borrow_column_mut();

        let mut dest = dest
            .downcast_ref::<ColumnInner<C>>()
            .ok_or(DbError::ColumnTypeDiscrepancy)?
            .borrow_column_mut();

        debug_assert!(from.len() < index);

        let component = from.remove(index);
        dest.push(component);
        Ok(dest.len())
    }

    /// Resizes the column to hold at least [min_size] components
    pub fn dynamic_resize(column: &AnyPtr, min_size: usize) -> usize {
        let real_column = column
            .downcast_ref::<ColumnInner<C>>()
            .expect("column type mismatch in dynamic method call");
        real_column.resize_minimum(min_size)
    }

    /// Constructs a [Column] and returns a type erased pointer to it
    pub fn dynamic_ctor() -> AnyPtr {
        Box::new(ColumnInner::<C>::new())
    }

    /// Creates a new instance of the component type C at the specified index in the column
    pub fn dynamic_instance(column: &AnyPtr, index: usize) {
        // TODO: Just trying to get this working - need some guard rails around here
        let real_column = column
            .downcast_ref::<ColumnInner<C>>()
            .expect("column type mismatch in dynamic method call");
        let mut column = real_column.borrow_column_mut();
        column[index] = Default::default();
    }

    pub fn resize_minimum(&self, min_size: usize) -> usize {
        let power_of_two_index = std::cmp::max(min_size, 1).next_power_of_two();
        debug_assert!(power_of_two_index < COLUMN_LENGTH_MAXIMUM);
        let mut column = self.borrow_column_mut();
        let len = column.len();
        if min_size >= len {
            column.resize_with(power_of_two_index, Default::default);
        }
        let new_len = column.len();
        new_len
    }
}
