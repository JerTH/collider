use std::any::TypeId;
use std::collections::HashMap;
use std::collections::HashSet;
use std::marker::PhantomData;

use crate::EntityId;
use crate::db::DataTableGuard;
use crate::db::EntityDatabase;
use crate::db::ComponentColumn;
use crate::comps::Component;
use crate::comps::ComponentType;
use crate::comps::ComponentTypeSet;
use crate::family::SubFamilies;
use crate::family::SubFamilyMap;
use crate::id::FamilyId;

#[derive(Debug)]
pub struct TransformSuccess;
#[derive(Debug)]
pub struct TransformError { }

pub type TransformResult = Result<TransformSuccess, TransformError>;

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
    fn run(data: Rows<Self::Data>) -> TransformResult;
}

trait Runs {
    fn run_on(&mut self, db: &EntityDatabase) -> TransformResult;
}

impl<'a, T> Runs for T
    where
        T: Transformation,
        T::Data: Selection,
{
    fn run_on(&mut self, db: &EntityDatabase) -> TransformResult
    {
        let rows = T::Data::as_row::<T::Data>();
        T::run(rows)
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



pub trait SelectOne<'a> {
    type Ref;
    
    type Iterator;
    
    fn select_one(db: &EntityDatabase) -> Self;
    
    fn iterate_with(db: &'a EntityDatabase, sub_families: SubFamilies) -> Self::Iterator;
    
    fn as_ref_type(&mut self) -> Self::Ref    {
        //self.into()
        todo!()
    }
}



/// How the consumer makes a selection. A selection is simply a
/// tuple of one or multiple Read<T> and Write<T> structures where
/// each T can be a different component type
pub trait Selection {
    fn rw_set() -> RwSet;
    fn arity() -> usize;
    fn make(db: &EntityDatabase) -> Self;
    fn as_row<'a, T>() -> Rows<'a, T>;
}



/// The reference side of a `Selection` used for iteration
/// Where a `Selection` is a tuple of `Read`'s and `Write`'s, a `Row
/// is a tuple of `ReadIter`'s and `WriteIter`'s. The iterators differ
/// from their counterparts in that they each actually hold a references
/// to the underlying database and thus have associated lifetimes
pub trait ImplRow<'a> {
    type IteratorTuple;
    fn from_selection(db: &'a EntityDatabase, select: impl Selection) -> Self::IteratorTuple;
}



pub struct Rows<'a, T> {
    db: &'a EntityDatabase,
    sub_families: SubFamilies,
    _p: PhantomData<T>,
}



#[derive(Default)]
pub struct Read<T: Component> {
    _p: PhantomData<T>,
}



impl<T: Component> Read<T> {
    pub(crate) fn new() -> Self {
        Self {
            _p: PhantomData::default()
        }
    }
}



impl<'a, C> SelectOne<'a> for Read<C>
    where 
        Self: 'a,
        C: Component,
{
    type Ref = &'a C;
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



#[derive(Default)]
pub struct Write<T: Component> {
    _p: PhantomData<T>,
}



impl<T: Component> Write<T> {
    pub(crate) fn new() -> Self {
        Self {
            _p: PhantomData::default()
        }
    }
}



impl<'a, C> SelectOne<'a> for Write<C>
    where
        Self: 'a,
        C: Component,
{
    type Ref = &'a mut C;

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



/// Iterates one component kind in read mode bounded by a selection
#[deprecated]
#[allow(dead_code)]
pub struct ReadIter<'a, T: Component> {
    db: &'a EntityDatabase,
    sub_families: SubFamilies,
    _p: PhantomData<T>,
}



/// Iterates one component kind in write mode bounded by a selection
#[deprecated]
#[allow(dead_code)]
pub struct WriteIter<'a, T: Component> {
    db: &'a EntityDatabase,
    sub_families: SubFamilies,
    _p: PhantomData<T>,
}



/// A special iterator which traverses rows of the
/// database which match a selection
pub struct RowIter<'a, T: ImplRow<'a>> {
    db: &'a EntityDatabase,
    next_entity: Option<EntityId>,
    table_guard: Option<DataTableGuard<'a>>,
    columns_iter: Option<T>, // TODO: Probably need some weird transmogrifying through traits
    family_iter: <SubFamilies as IntoIterator>::IntoIter,
    _p: PhantomData<&'a T>
}



impl<'a, T: ImplRow<'a>> RowIter<'a, T> {
    fn new(db: &'a EntityDatabase, sub_families: SubFamilies) -> Self {
        let sub_family_iter = sub_families.into_iter();
        RowIter {
            db,
            next_entity: None,
            table_guard: None,
            columns_iter: todo!(),
            family_iter: sub_family_iter,
            _p: PhantomData::default()
        }
    }
}



pub trait Reads<C: Component> {}



pub trait Writes<C: Component> {}



trait Guards {}



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


/// Here be macro dragons
/// 
/// These macros are what make selections possible and ergonomic. They
/// expand into implementations for arbitrary user defined tuple
/// combinations which represent concrete selections into the database
/// With a large amount of components or selection kinds, this will incur
/// some compilation overhead, the trade-off however is that database
/// access is fast and predictable
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

            fn as_row<'b, T>() -> Rows<'b, T> {
                todo!()
            }
        }
        
        impl<'a, $($t,)+> ImplRow<'a> for ($($t,)+)
            where
                ($($t,)+): Selection,
                $($t: Metadata,)+
                $($t: SelectOne<'a>,)+
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

        impl<'a, $($t,)+> Iterator for RowIter<'a, ($($t,)+)>
            where
                $($t: Metadata,)+
                $($t: SelectOne<'a>,)+
                $($t: Component,)+
                //($($t,)+): ImplRow<'a>,
        {
            type Item = ($($t::Ref,)+);
            
            fn next(&mut self) -> Option<Self::Item> {
                // just make it work even if it's slow for now
                loop {
                    if let Some(guard) = self.table_guard.as_mut() {
                        // we have a locked table
                        // build the row here
                        let row = (
                            $(
                                guard.get_mut(&component!($t))?
                                     .data.downcast_mut::<ComponentColumn<$t>>()?
                                     .get_mut(&self.next_entity?)?
                                     .as_ref_type()
                            ,)+
                        );
                        // set the next entity

                        return Some(row)
                    } else {
                        // we don't have a locked table, do we have a next family?
                        match self.family_iter.next() {
                            Some(family_id) => {
                                // spin until we acquire the data table we're interested in
                                // there are much better ways of doing these worth exploring
                                // but this works for now
                                loop {
                                    if let Some(guard) = self.db.try_lock_family_table(&family_id) {
                                        self.table_guard = Some(guard);
                                        break;
                                    } else {
                                        std::thread::yield_now();
                                    }
                                }
                            },
                            None => {
                                // no table and no next family - iteration is finished
                                return None;
                            }
                        }
                    }
                }
            }
        }
        
        #[allow(unreachable_code)]
        impl<'a, $($t,)+> IntoIterator for Rows<'a, ($($t,)+)>
            where
                $($t: Metadata,)+
                $($t: SelectOne<'a>,)+
                $($t: Component,)+
                //($($t,)+): ImplRow<'a>,
                $($t: 'a,)+
        {
            type Item = ($($t::Ref,)+);

            type IntoIter = RowIter<'a, ($($t,)+)>;

            fn into_iter(self) -> Self::IntoIter {
                let next_entity = None;
                let table_guard = None;
                let family_iter = self.sub_families.into_iter();
                let columns_iter = todo!();

                RowIter {
                    db: self.db,
                    next_entity,
                    table_guard,
                    columns_iter,
                    family_iter,
                    _p: PhantomData::default(),
                }
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

