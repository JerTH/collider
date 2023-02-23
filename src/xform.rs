use std::any::TypeId;
use std::collections::HashMap;
use std::collections::HashSet;
use std::marker::PhantomData;

use crate::db::EntityDatabase;
use crate::comps::Component;
use crate::comps::ComponentType;
use crate::comps::ComponentTypeSet;
use crate::family::SubFamilies;
use crate::family::SubFamilyMap;

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
    type Data: Selection;
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

/// Selection-level meta data associated with a component
trait Metadata {
    fn reads() -> Option<ComponentType> { None }
    fn writes() -> Option<ComponentType> { None }
    fn component_type() -> ComponentType;
}

impl<'a, T> Metadata for Read<T> where T: Component {
    fn reads() -> Option<ComponentType> {
        Some(ComponentType::of::<T>())
    }

    fn component_type() -> ComponentType {
        ComponentType::of::<T>()
    }
}

impl<'a, T> Metadata for Write<T> where T: Component {
    fn writes() -> Option<ComponentType> {
        Some(ComponentType::of::<T>())
    }

    fn component_type() -> ComponentType {
        ComponentType::of::<T>()
    }
}

pub(crate) trait SelectOne<'a> {
    type Inner;
    type Iterator;
    fn select_one(db: &EntityDatabase) -> Self;
    fn iterate_with(db: &'a EntityDatabase, sub_families: SubFamilies) -> Self::Iterator;
}

impl<'a, C> SelectOne<'a> for Read<C>
    where 
        Self: 'a,
        C: Component,
{
    type Inner = C;
    type Iterator = ReadIter<'a, C>;

    fn select_one(db: &EntityDatabase) -> Self {
        db.select_read::<C>()
    }

    fn iterate_with(db: &'a EntityDatabase, sub_families: SubFamilies) -> Self::Iterator {
        ReadIter {
            db,
            sub_families,
            _p: PhantomData::default(),
        }
    }
}

impl<'a, C> SelectOne<'a> for Write<C>
    where
        Self: 'a,
        C: Component,
{
    type Inner = C;
    type Iterator = WriteIter<'a, C>;

    fn select_one(db: &EntityDatabase) -> Self {
        db.select_write::<C>()
    }

    fn iterate_with(db: &'a EntityDatabase, sub_families: SubFamilies) -> Self::Iterator {
        WriteIter {
            db,
            sub_families,
            _p: PhantomData::default(),
        }
    }
}

/// How the consumer makes a selection. A selection is simply a
/// tuple of one or multiple Read<T> and Write<T> structures where
/// each T can be a different component type
pub trait Selection {
    fn rw_set() -> RwSet;
    fn arity() -> usize;
    fn make(db: &EntityDatabase) -> Self;
}

/// The reference side of a `Selection` used for iteration
/// Where a `Selection` is a tuple of `Read`'s and `Write`'s, a `Row
/// is a tuple of `ReadIter`'s and `WriteIter`'s. The iterators differ
/// from their counterparts in that they each actually hold a references
/// to the underlying database and thus have associated lifetimes
trait Row<'a> {
    type IteratorTuple;
    fn from_selection(db: &'a EntityDatabase, select: impl Selection) -> Self::IteratorTuple;
}

pub struct Read<T: Component> {
    _p: PhantomData<T>,
}

pub struct Write<T: Component> {
    _p: PhantomData<T>,
}

/// Iterates one component kind bounded by a selection
pub struct ReadIter<'a, T: Component> {
    db: &'a EntityDatabase,
    sub_families: SubFamilies,
    _p: PhantomData<T>,
}

pub struct WriteIter<'a, T: Component> {
    db: &'a EntityDatabase,
    sub_families: SubFamilies,
    _p: PhantomData<T>,
}

/// A special iterator which traverses a collection of
/// read and write iterators as one, the effect is this
/// iterates each row of the database which matches the
/// selection that generated it
struct RowIter<'a, T> {
    locks: Locks<T>,
    _p: PhantomData<&'a T>
}

pub trait Reads<C: Component> {}

pub trait Writes<C: Component> {}

struct Locks<T> {
    _p: PhantomData<T>,
}

trait IntoRowIterator {
    type Item;
    type IntoIter;
    fn into_iter(self) -> Self::IntoIter;
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

/// Here be dragons
/// 
/// These macros are what make selections possible and ergonomic. They
/// expand into implementations for arbitrary user defined tuple
/// combinations which represent concrete selections into the database
/// With a large amount of components or selection kinds, this will incur
/// some compilation overhead
macro_rules! impl_tdata_tuple {
    ($($t:tt),+) => {
        impl<'a, $($t,)+> Selection for ($($t,)+)
            where
                $($t: Metadata,)+
                $($t: SelectOne<'a>,)+
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
        
        impl<'a, $($t,)+> Row<'a> for ($($t,)+)
            where
                $($t: SelectOne<'a>,)+
                $($t: Metadata,)+
        {
            type IteratorTuple = ($($t::Iterator,)+);

            fn from_selection(db: &'a EntityDatabase, select: impl Selection) -> Self::IteratorTuple {
                let i = [
                    $(
                        $t::component_type()
                    ),+
                ].into_iter();
                let set = ComponentTypeSet::from_iter(i);

                let sf: crate::family::SubFamilies = db.sub_families(set).expect("expected sub families");
                ($(
                    $t::iterate_with(db, sf.clone()),
                )+)
            }
        }
    };
}

macro_rules! one {
    ($t:tt) => { 1usize };
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

/*
 * Status: 
 * The transform contains a set of (Read<A>, Write<B>, ..) which implements Selection
 * So far we can succesfully produce that set using trait level polymorphism
 * We need to be able to iterate the Read and Write structures, they need to strictly
 * iterate only families where they are all present - lets do that first
 * 
 */