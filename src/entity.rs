//! Entity
//! 
//! Functionality and support for entities

use collider_core::*;
use std::{fmt::Display, error::Error, sync::{PoisonError, atomic::{AtomicU32, Ordering}, Mutex}};

#[derive(Debug)]
pub enum EntityAllocError {
    PoisonedFreeList,
}

impl Display for EntityAllocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityAllocError::PoisonedFreeList => write!(f, "poisoned free list mutex"),
        }
    }
}

impl Error for EntityAllocError {}

type EntityAllocResult = Result<EntityId, EntityAllocError>;

impl<T> std::convert::From<PoisonError<T>> for EntityAllocError {
    fn from(_: PoisonError<T>) -> Self {
        EntityAllocError::PoisonedFreeList
    }
}

pub trait FreeEntityWith<T> {
    fn free(&self, free: T) -> Result<(), EntityAllocError>;
}

#[derive(Debug)]
pub struct EntityAllocator {
    count: AtomicU32,
    free: Mutex<Vec<EntityId>>,
}

impl EntityAllocator {
    pub fn new() -> Self {
        Self {
            count: AtomicU32::new(0),
            free: Default::default(),
        }
    }

    pub fn alloc(&self) -> Result<EntityId, EntityAllocError> {
        let mut guard = self.free.lock()?;

        match guard.pop() {
            Some(id) => Ok(id.next_generation()),
            None => {
                let count = self.count.fetch_add(1u32, Ordering::SeqCst);
                Ok(EntityId::from(count))
            }
        }
    }
}

impl<I> FreeEntityWith<I> for EntityAllocator
where
    I: Iterator<Item = EntityId>,
{
    fn free(&self, free: I) -> Result<(), EntityAllocError> {
        let mut guard = self.free.lock()?;
        for item in free {
            guard.push(item)
        }
        Ok(())
    }
}
