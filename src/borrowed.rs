//! Borrowed
//! 
//! This module is responsible for safe, non-aliased, and concurrent access
//! to table columns


use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, Ordering};

const ONE_SHARED_BORROW: isize = 1isize;
const NOT_BORROWED: isize = 0isize;
const MUTABLE_BORROW: isize = -1isize;

#[derive(Default, Debug, Clone)]
pub struct BorrowSentinel(Arc<AtomicIsize>);

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

pub struct BorrowRef {
    sentinel: BorrowSentinel,
}

const ASCEND_PANIC_MSG: &str = "attempted to lift immutable reference into a mutable reference when more than one reference exist";

impl BorrowRef {
    pub fn new(borrow: BorrowSentinel) -> Option<Self> {
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
                        return Some(BorrowRef { sentinel: borrow });
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
    pub fn ascend(self) -> BorrowRefMut {
        let cur = self.sentinel.load(Ordering::SeqCst);
        let new = MUTABLE_BORROW;
        
        if cur != ONE_SHARED_BORROW {
            panic!("{}", crate::borrowed::ASCEND_PANIC_MSG);
        }

        match self.sentinel.compare_exchange(
            cur, new,
            Ordering::SeqCst,
            Ordering::SeqCst
        ) {
            Ok(_) => {
                // Here we take the value of `self.sentinel` WITHOUT dropping `self`, 
                // to avoid trigging our custom drop code which would subsequently modify
                // our sentinel value undesireably
                let sentinel = unsafe { std::ptr::read_unaligned(&self.sentinel) };
                std::mem::forget(self);
                
                return BorrowRefMut {
                    sentinel,
                }
            },
            Err(_) => {
                panic!("{}", crate::borrowed::ASCEND_PANIC_MSG);
            },
        }
    }
}

impl Drop for BorrowRef {
    fn drop(&mut self) {
        let borrow = self.sentinel.clone();
        #[cfg(debug_assertions)]
        {
            let cur = borrow.load(Ordering::SeqCst);
            debug_assert!(!is_mut_borrow(cur));
        }
        borrow.fetch_sub(1isize, Ordering::SeqCst);
    }
}

pub struct BorrowRefMut {
    sentinel: BorrowSentinel
}

impl BorrowRefMut {
    pub fn new(borrow: BorrowSentinel) -> Option<Self> {
        let cur = NOT_BORROWED;
        let new = MUTABLE_BORROW;
        match borrow.compare_exchange(
                cur, new, 
                Ordering::SeqCst, 
                Ordering::SeqCst
            ) {
            Ok(_) => {
                return Some(BorrowRefMut { sentinel: borrow });
            },
            Err(_) => {
                return None
            },
        }
    }
}

impl<'b> Drop for BorrowRefMut {
    fn drop(&mut self) {
        let borrow = self.sentinel.clone();
        #[cfg(debug_assertions)]
        {
            let cur = borrow.load(Ordering::SeqCst);
            debug_assert!(is_mut_borrow(cur));
        }
        borrow.fetch_add(1isize, Ordering::SeqCst);
    }
}

pub enum RawBorrow {
    Immutable(BorrowRef),
    Mutable(BorrowRefMut),
}

#[derive(Debug)]
pub enum BorrowError {
    AlreadyBorrowed,
}
