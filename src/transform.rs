//! Transformations
//! 
//! Functionality related to transforming data contained in an
//! [crate::database::reckoning::EntityDatabase]

use crate::database::ComponentType;
use crate::conflict::ConflictGraph;
use crate::conflict::Dependent;
use crate::database::reckoning::AnyPtr;
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;

use crate::database::Component;
use crate::database::EntityDatabase;
use crate::id::FamilyId;
use crate::id::FamilyIdSet;

pub struct Phase {
    subphases: Vec<HashMap<TransformationId, DynTransform>>,
}

impl Phase {
    pub fn new() -> Self {
        Phase {
            subphases: Vec::new(),
        }
    }

    pub fn add_transformation<T>(&mut self, tr: T)
    where
        T: Transformation,
    {
        // TODO:
        // Rebuilding the graph with every insert is super inefficient,
        // some sort of from_iter implementation or a defered building
        // of the conflict graph would help here

        let reads = T::Data::READS
            .iter()
            .cloned()
            .filter_map(|item| item)
            .collect();
        let writes = T::Data::WRITES
            .iter()
            .cloned()
            .filter_map(|item| item)
            .collect();

        let dyn_transform = DynTransform {
            ptr: Box::new(tr),
            reads,
            writes,
        }; // ========== Inject Read/Write requirements here

        let transform_tuple = (T::id(), dyn_transform);

        // Resolve conflicts each time we add a transformation
        let transforms: Vec<(TransformationId, DynTransform)> = self
            .subphases
            .drain(..)
            .flatten()
            .chain([transform_tuple].into_iter())
            .collect();

        let mut graph = ConflictGraph::new();

        transforms.into_iter().for_each(|(k, v)| graph.insert(k, v));

        let deconflicted = graph.build();

        deconflicted.into_iter().for_each(|bucket| {
            let subphase: HashMap<TransformationId, DynTransform> =
                HashMap::from_iter(bucket.into_iter());
            self.subphases.push(subphase);
        });
    }
    
    pub fn run_on(&mut self, db: &EntityDatabase) -> PhaseResult {
        for subphase in self.subphases.iter_mut() {
            
            let mut subphase_results = Vec::new();
            for (id, dyn_transformation) in subphase {
                let transform_result = dyn_transformation.ptr.run_on(db);
                subphase_results.push((id, transform_result));
            }
            println!("subphase results:\n");
            println!("{:#?}\n\n", subphase_results);
            // TODO: engage multithreading here
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransformationId(std::any::TypeId);

pub trait Transformation: 'static {
    type Data: Selection;
    
    fn run(data: Rows<Self::Data>) -> TransformationResult;
    
    fn messages(_: Messages) {
        todo!()
    }

    /// Returns a unique identifier for a given transformation impl
    fn id() -> TransformationId
    where
        Self: 'static,
    {
        TransformationId(std::any::TypeId::of::<Self>())
    }
}

pub trait Runs {
    fn run_on(&self, db: &EntityDatabase) -> TransformationResult;
}

impl<RTuple> Runs for RTuple
where
    RTuple: Transformation,
    RTuple::Data: Selection,
{
    fn run_on(&self, db: &EntityDatabase) -> TransformationResult {
        let rows = RTuple::Data::as_rows::<RTuple::Data>(db);
        RTuple::run(rows)
    }
}

struct DynTransform {
    ptr: Box<dyn Runs>,
    reads: Vec<ComponentType>,
    writes: Vec<ComponentType>,
}

impl Dependent for DynTransform {
    type Dependency = ComponentType;

    fn dependencies<'iter>(&'iter self) -> impl Iterator<Item = <DynTransform as Dependent>::Dependency> + 'iter {
        self.reads.iter().cloned()
    }

    fn exclusive_dependencies<'iter>(
        &'iter self,
    ) -> impl Iterator<Item = <DynTransform as Dependent>::Dependency> + 'iter {
        self.writes.iter().cloned()
    }
}

pub struct Read<C: Component> {
    marker: PhantomData<C>,
}
pub struct Write<C: Component> {
    marker: PhantomData<C>,
}

#[const_trait]
pub trait ReadWrite {}
impl<C> const ReadWrite for Read<C> where C: Component {}
impl<C> const ReadWrite for Write<C> where C: Component {}

#[const_trait]
pub trait MetaData {}
impl<'db, C> const MetaData for Read<C> where C: Component {}
impl<'db, C> const MetaData for Write<C> where C: Component {}

pub struct RowIter<'db, RTuple> {
    pub(crate) db: &'db EntityDatabase,
    pub(crate) family: FamilyId,
    pub(crate) marker: PhantomData<RTuple>,
}

impl<'db, RTuple> RowIter<'db, RTuple> {
    pub fn new(db: &'db EntityDatabase, family: FamilyId) -> Self {
        Self {
            db,
            family,
            marker: PhantomData::default(),
        }
    }
}

#[const_trait]
pub trait SelectOne<'db> {
    type Ref;
    type Type;
    fn reads() -> Option<ComponentType> {
        None
    }
    fn writes() -> Option<ComponentType> {
        None
    }
}

impl<'db, C> const SelectOne<'db> for Read<C>
where
    Self: 'db,
    C: Component,
{
    type Type = C;
    type Ref = &'db Self::Type;

    fn reads() -> Option<ComponentType> {
        Some(ComponentType::of::<Self::Type>())
    }
}

impl<'db, C> const SelectOne<'db> for Write<C>
where
    Self: 'db,
    C: Component,
{
    type Type = C;
    type Ref = &'db mut Self::Type;

    fn writes() -> Option<ComponentType> {
        Some(ComponentType::of::<Self::Type>())
    }
}

#[const_trait]
pub trait Selection {
    fn as_rows<'db, T>(db: &'db EntityDatabase) -> Rows<'db, T> {
        #![allow(unreachable_code, unused_variables)]
        unimplemented!()
        //crate::database::Rows {
        //    db,
        //    fs,
        //    marker: PhantomData::default(),
        //}
    }

    const READS: &'static [Option<ComponentType>];
    const WRITES: &'static [Option<ComponentType>];
}

pub struct Rows<'db, RTuple> {
    db: &'db EntityDatabase,
    fs: FamilyIdSet,
    marker: PhantomData<RTuple>,
}

impl<'db, RTuple> Rows<'db, RTuple> {
    pub fn database(&self) -> &'db EntityDatabase {
        self.db
    }
    pub fn families(&self) -> FamilyIdSet {
        self.fs.clone()
    }
}

pub struct Messages {}
pub trait RwData {}
pub struct ReadIter {}
pub struct WriteIter {}
pub trait Reads {}
pub trait Writes {}
pub struct RwSet {}

#[derive(Debug)]
pub enum TransformationError {}
pub type TransformationResult = Result<(), TransformationError>;
pub type PhaseResult = Result<(), Vec<TransformationError>>;
