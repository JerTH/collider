use std::any::TypeId;
use std::collections::HashMap;

use crate::EntityDatabase;
use crate::Component;

#[derive(Debug)]
pub struct TransformSuccess;
#[derive(Debug)]
pub enum TransformError { }

type TransformResult = Result<TransformSuccess, TransformError>;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
struct TransformId(TypeId);

pub(in self) trait ImplTransformId {
    fn id() -> TransformId;
}

impl<T> ImplTransformId for T where T: Transformation {
    fn id() -> TransformId {
        TransformId(std::any::TypeId::of::<T>())
    }
} 

/// A `Transformation` defines a single set of read/write logic to be run on an `EntityDatabase`
/// 
/// Transformations may run in parallel, or in sequence, as determined by the arrangement of
/// the phases they reside in, as well as the internal contention within a `Phase`
pub trait Transformation: 'static {
    type Data;
    fn run(data: Self::Data) -> TransformResult;
}

trait Runs {
    fn run_on(&mut self, db: &EntityDatabase) -> TransformResult;
}

impl<T> Runs for T where T: Transformation {
    fn run_on(&mut self, db: &EntityDatabase) -> TransformResult {
        let data = db.select::<T::Data>();
        T::run(data)
    }
}

struct DynTransform {
    fn_ptr: Box<dyn Runs>,
}

impl DynTransform {
    fn new<T: Transformation>(transform: T) -> Self {
        DynTransform { fn_ptr: Box::new(transform) }
    }

    fn run(&mut self, db: &EntityDatabase) -> TransformResult {
        self.fn_ptr.run_on(db)
    }
}

/// A `Phase` is comprised of a set of transformations. Phases delineate execution boundaries
/// in simulation logic. One phase at a time is executed on an `EntityDatabase`, but, the individual
/// transformations in a phase have no guaranteed execution order. Indeed, they may be executed in
/// parallel
pub struct Phase {
    transforms: HashMap<TransformId, DynTransform>,
}

impl Phase {
    pub fn new() -> Self {
        Phase {
            transforms: Default::default()
        }
    }

    pub fn run_on(&mut self, db: &EntityDatabase) -> TransformResult {
        for (_id, transform) in self.transforms.iter_mut() {
            transform.run(db)?;
        }
        Ok(TransformSuccess)
    }

    pub fn add_transformation<T: Transformation>(&mut self, transform: T) {
        let id = T::id();
        let transform = DynTransform::new(transform);
        self.transforms.insert(id, transform);
    }
}

pub struct Read<T: Component> {
    _phantom: std::marker::PhantomData<T>,
}

pub struct Write<T: Component> {
    _phantom: std::marker::PhantomData<T>,
}
