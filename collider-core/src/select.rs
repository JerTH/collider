//!
//! 
//! Selects
//! 
//! 

use crate::{indexing::IndexQuery, component::ComponentType};

pub trait Selection {
    
}

/// Trait which describes all the components something is interested in
pub trait Selects {
    const READS: &'static [ComponentType];
    const WRITES: &'static [ComponentType];
    const GLOBAL: &'static [ComponentType];

    fn reads() -> Vec<ComponentType> {
        [Self::READS, Self::WRITES].concat()
    }
    
    fn writes() -> Vec<ComponentType> {
        Self::WRITES.to_vec()
    }
}

#[const_trait]
pub trait SelectOne<'db> {
    type Ref;
    type Type;
    type BorrowType;

    //const READS: &'static [ComponentType];
    //const WRITES: &'static [ComponentType];

    //fn reads() -> &'static [ComponentType] { Self::READS }
    //fn writes() -> &'static [ComponentType] { Self::WRITES }
}

/// Blanket implementation for IndexQuery types
impl<'db, T> const SelectOne<'db> for T
where
    T: IndexQuery + 'db,
    T: Selects,
{
    type Ref = &'db Self;
    type Type = Self;
    type BorrowType = &'db Self;

    //const READS: &'static [ComponentType] = <T as Selects>::READS;
    //const WRITES: &'static [ComponentType] = &[];
}

pub trait DatabaseSelection: IntoIterator + Selects {}

pub trait SelectionField {}
