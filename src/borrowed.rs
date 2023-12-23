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

const ONE_BORROW: isize = 1isize;
const NOT_BORROWED: isize = 0isize;
const MUTABLE_BORROW: isize = -1isize;

#[derive(Default, Debug)]
pub struct BorrowSentinel(AtomicIsize);
impl BorrowSentinel {
    pub fn new() -> Self {
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
pub fn is_mut_borrow(value: isize) -> bool {
    value < NOT_BORROWED
} 

pub struct BorrowRef<'b> {
    borrow: &'b BorrowSentinel
}

const ASCEND_PANIC_MSG: &str = "attempted to lift immutable reference into a mutable reference when more than one reference exist";

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

    
    /// Ascends the immutable borrow into a mutable borrow, ONLY IF 
    /// this is the only existing immutable borrow
    pub fn ascend(self) -> BorrowRefMut<'b> {
        let cur = self.borrow.load(Ordering::SeqCst);
        let new = MUTABLE_BORROW;
        
        if cur > ONE_BORROW {
            panic!("{}", crate::borrowed::ASCEND_PANIC_MSG);
        }

        match self.borrow.compare_exchange(
            cur, new,
            Ordering::SeqCst,
            Ordering::SeqCst
        ) {
            Ok(_) => {
                return BorrowRefMut {
                    borrow: self.borrow,
                }
            },
            Err(_) => {
                panic!("{}", crate::borrowed::ASCEND_PANIC_MSG);
            },
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

pub struct BorrowRefMut<'b> {
    borrow: &'b BorrowSentinel
}

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

pub enum BorrowRefEither<'b> {
    Immutable(BorrowRef<'b>),
    Mutable(BorrowRefMut<'b>),
}

#[derive(Debug)]
pub enum BorrowError {
    AlreadyBorrowed,
}
