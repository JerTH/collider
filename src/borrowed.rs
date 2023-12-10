//! Borrowed
//! 
//! This module is responsible for safe, non-aliased, and concurrent access
//! to table columns


use std::cell::UnsafeCell;
use std::fmt::Display;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicIsize, Ordering};
use crate::EntityId;
use crate::database::reckoning::{AnyPtr, Component, DbError};

const NOT_BORROWED: isize = 0isize;
const MUTABLE_BORROW: isize = -1isize;

#[derive(Default, Debug)]
pub struct BorrowSentinel(AtomicIsize);
impl BorrowSentinel {
    fn new() -> Self {
        Default::default()
    }
}

impl Deref for BorrowSentinel {
    type Target = AtomicIsize;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[inline]
fn is_mut_borrow(value: isize) -> bool {
    value < NOT_BORROWED
} 

pub struct BorrowRef<'b> { borrow: &'b BorrowSentinel }
impl<'b> BorrowRef<'b> {
    pub fn new(borrow: &'b BorrowSentinel) -> Option<Self> {
        loop {
            let cur = borrow.load(Ordering::SeqCst);
            let new = cur + 1;
            if is_mut_borrow(new) {
                return None;
            } else {
                match borrow.compare_exchange(
                    cur, new, 
                    Ordering::SeqCst, 
                    Ordering::SeqCst
                ) {
                    Ok(_) => {
                        return Some(BorrowRef { borrow });
                    },
                    Err(_) => {
                        continue; // someone else likely interacted with this borrow, try again
                    },
                }
            }
        }
    }
}
impl<'b> Drop for BorrowRef<'b> {
    fn drop(&mut self) {
        let borrow = self.borrow;
        #[cfg(debug_assertions)]
        {
            let cur = borrow.load(Ordering::SeqCst);
            debug_assert!(!is_mut_borrow(cur));
        }
        borrow.fetch_sub(1isize, Ordering::SeqCst);
    }
}
pub struct BorrowRefMut<'b> { borrow: &'b BorrowSentinel }
impl<'b> BorrowRefMut<'b> {
    pub fn new(borrow: &'b BorrowSentinel) -> Option<Self> {
        let cur = NOT_BORROWED;
        let new = MUTABLE_BORROW;
        match borrow.compare_exchange(
                cur, new, 
                Ordering::SeqCst, 
                Ordering::SeqCst
            ) {
            Ok(_) => {
                return Some(BorrowRefMut { borrow });
            },
            Err(_) => {
                return None
            },
        }
    }
}
impl<'b> Drop for BorrowRefMut<'b> {
    fn drop(&mut self) {
        let borrow = self.borrow;
        #[cfg(debug_assertions)]
        {
            let cur = borrow.load(Ordering::SeqCst);
            debug_assert!(is_mut_borrow(cur));
        }
        borrow.fetch_add(1isize, Ordering::SeqCst);
    }
}
#[derive(Debug)]
pub enum BorrowError {
    AlreadyBorrowed,
}
type ColumnType<C: Component> = Vec<C>;
pub struct ColumnRef<'b, C> {
    column: NonNull<ColumnType<C>>,
    borrow: BorrowRef<'b>,
}
impl<'b, C> ColumnRef<'b, C> {
    pub fn new(column: NonNull<ColumnType<C>>, borrow: BorrowRef<'b>) -> Self {
        Self { column, borrow }
    }
}
impl<C> Deref for ColumnRef<'_, C> {
    type Target = ColumnType<C>;
    fn deref(&self) -> &Self::Target {
        // SAFETY
        // Safe to access because we hold a runtime checked borrow
        unsafe { self.column.as_ref() }
    }
}
impl<'b, C: 'b> IntoIterator for ColumnRef<'b, C> {
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
            invariant: PhantomData::default()
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
const COLUMN_LENGTH_MAXIMUM: usize = 2^14;
/// The owner of the actual data we are interested in. 
#[derive(Default, Debug)]
pub struct Column<C: Component> {
    /// INVARIANT: 
    /// 
    /// For an entity in a table, its associated components must
    /// always occupy the same index in each column. Failure to
    /// uphold this invariant will result in undefined behavior
    values: UnsafeCell<Vec<C>>,
    borrow: BorrowSentinel,
}
impl<C: Component> Display for Column<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Column\n")?;
        write!(f, "\t{:?}\n", self.borrow)?;
        write!(f, "\t{:?}\n", unsafe { &*self.values.get() } )
    }
}

impl<'b, C: Component> Column<C> {
    pub fn new() -> Self {
        Self {
            values: UnsafeCell::new(Vec::new()),
            borrow: BorrowSentinel::new(),
        }
    }
    
    pub fn borrow_column(&'b self) -> ColumnRef<'b, C> {
        self.try_borrow().expect("column was already mutably borrowed")
    }

    fn try_borrow(&'b self) -> Result<ColumnRef<'b, C>, BorrowError> {
        match BorrowRef::new(&self.borrow) {
            Some(borrow) => {
                let column = unsafe {
                    NonNull::new_unchecked(self.values.get())
                };
                Ok(ColumnRef::new(column, borrow))
            },
            None => {
                Err(BorrowError::AlreadyBorrowed)
            },
        }
    }
    
    pub fn borrow_column_mut(&'b self) -> ColumnRefMut<'b, C> {
        self.try_borrow_mut().expect("column was already borrowed")
    }
    
    fn try_borrow_mut(&'b self) -> Result<ColumnRefMut<'b, C>, BorrowError> {
        match BorrowRefMut::new(&self.borrow) {
            Some(borrow) => {
                let column = unsafe {
                    NonNull::new_unchecked(self.values.get())
                };
                Ok(ColumnRefMut::new(column, borrow))
            },
            None => {
                Err(BorrowError::AlreadyBorrowed)
            },
        }
    }
    
    /// Moves a single component from one [Column] to another, if they are the same type
    pub fn dynamic_move(entity: &EntityId, index: usize, from: &AnyPtr, dest: &AnyPtr) -> Result<usize, DbError> {
        let mut from = from.downcast_ref::<Column<C>>()
            .ok_or(DbError::ColumnTypeDiscrepancy)?.borrow_column_mut();
        
        let mut dest = dest.downcast_ref::<Column<C>>()
            .ok_or(DbError::ColumnTypeDiscrepancy)?.borrow_column_mut();
        
        debug_assert!(from.len() < index);
        
        let component = from.remove(index);
        dest.push(component);
        Ok(dest.len())
    }
    /// Resizes the column to hold at least [min_size] components
    pub fn dynamic_resize(column: &AnyPtr, min_size: usize) -> usize {
        let real_column = column
            .downcast_ref::<Column<C>>()
            .expect("column type mismatch in dynamic method call");
        real_column.resize_minimum(min_size)
    }
    
    /// Constructs a [Column] and returns a type erased pointer to it
    pub fn dynamic_ctor() -> AnyPtr {
        Box::new(Column::<C>::new())
    }
    /// Creates a new instance of the component type C at the specified index in the column
    pub fn dynamic_instance(column: &AnyPtr, index: usize) {
        // TODO: Just trying to get this working - need some guard rails around here
        let real_column = column
            .downcast_ref::<Column<C>>()
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