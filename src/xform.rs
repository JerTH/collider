use std::any::TypeId;
use std::collections::HashMap;
use std::collections::HashSet;
use std::marker::PhantomData;

use crate::EntityDatabase;
use crate::Component;
use crate::comps::ComponentType;

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
    type Data: TransformData;
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

pub trait Reads<C: Component> {}
pub trait Writes<C: Component> {}

impl<T> Reads<T> for Read<T> where T: Component {}
impl<T> Writes<T> for Write<T> where T: Component {}

pub struct ReadIter<C> { _phantom: PhantomData<C> }
pub struct WriteIter<C> { _phantom: PhantomData<C> }

pub trait SelectOne {
    type Iterator;
    type Inner;
    fn select_one(db: &EntityDatabase) -> Self;
}

impl<C> SelectOne for Read<C>
    where 
        C: Component,
{
    type Iterator = ReadIter<C>;
    type Inner = C;

    fn select_one(db: &EntityDatabase) -> Self {
        db.select_read::<C>()
    }
}

impl<C> SelectOne for Write<C>
    where
        C: Component,
{
    type Iterator = WriteIter<C>;
    type Inner = C;

    fn select_one(db: &EntityDatabase) -> Self {
        db.select_write::<C>()
    }
}

trait Metadata {
    fn reads() -> Option<ComponentType> { None }
    fn writes() -> Option<ComponentType> { None }
}

impl<T> Metadata for Read<T> where T: Component {
    fn reads() -> Option<ComponentType> {
        Some(ComponentType::of::<T>())
    }
}

impl<T> Metadata for Write<T> where T: Component {
    fn writes() -> Option<ComponentType> {
        Some(ComponentType::of::<T>())
    }
}

#[derive(Debug)]
pub struct RwSet {
    r: HashSet<ComponentType>,
    w: HashSet<ComponentType>,
}

impl RwSet {
    pub fn reads(&self) -> &HashSet<ComponentType> {
        &self.r
    }

    pub fn writes(&self) -> &HashSet<ComponentType> {
        &self.w
    }
}

pub trait TransformData {
    fn rw_set() -> RwSet;
    fn arity() -> usize;
    fn make(db: &EntityDatabase) -> Self;
}

macro_rules! one {
    ($t:tt) => { 1usize };
}

macro_rules! impl_tdata_tuple {
    ($($t:tt),+) => {
        impl<$($t,)+> TransformData for ($($t,)+)
            where
                $($t: Metadata,)+
                $($t: SelectOne,)+
        {
            fn rw_set() -> RwSet {
                let rset: HashSet<ComponentType> = vec![$($t::reads(),)+]
                    .into_iter()
                    .flatten()
                    .collect();

                let wset: HashSet<ComponentType> = vec![$($t::writes(),)+]
                    .into_iter()
                    .flatten()
                    .collect();

                RwSet {
                    r: rset,
                    w: wset,
                }
            }

            fn arity() -> usize {
                [
                    $(
                        one!($t)
                    ),+
                ].len()
            }

            fn make(db: &EntityDatabase) -> Self
            {
                ($($t::select_one(&db),)+)
            }
        }
    };
}

impl_tdata_tuple!(A);
impl_tdata_tuple!(A, B);
impl_tdata_tuple!(A, B, C);
impl_tdata_tuple!(A, B, C, D);
impl_tdata_tuple!(A, B, C, D, E);
impl_tdata_tuple!(A, B, C, D, E, F);
impl_tdata_tuple!(A, B, C, D, E, F, G);
impl_tdata_tuple!(A, B, C, D, E, F, G, H);
impl_tdata_tuple!(A, B, C, D, E, F, G, H, I);
impl_tdata_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_tdata_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_tdata_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);
