//! Transformations
//! 
//! Functionality related to transforming data contained in an
//! [crate::database::reckoning::EntityDatabase]

use crate::borrowed::RawBorrow;
use crate::column::ColumnKey;
use crate::column::RawColumnRef;
use crate::column::RawColumnRefMut;
use crate::components::Component;
use crate::components::ComponentType;
use crate::components::ComponentTypeSet;
use crate::conflict::ConflictGraph;
use crate::conflict::Dependent;
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::os::raw::c_void;
use std::ptr::NonNull;

use crate::database::EntityDatabase;
use crate::id::FamilyId;
use crate::id::FamilyIdSet;

pub struct Phase<'db> {
    subphases: Vec<HashMap<TransformationId, DynTransform<'db>>>,
}

impl<'db> Phase<'db> {
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
        // On the other hand, it really shouldn't happen much after the
        // database is initialized and most/all transformations are loaded

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
        };

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
    
    pub fn run_on(&mut self, db: &'db EntityDatabase) -> PhaseResult {
        for subphase in self.subphases.iter_mut() {
            
            let mut subphase_results = Vec::new();
            for (id, dyn_transformation) in subphase {
                let transform_result = dyn_transformation.ptr.run_on(db);
                subphase_results.push((id, transform_result));
            }
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

pub trait Runs<'db> {
    fn run_on(&self, db: &'db EntityDatabase) -> TransformationResult;
}

impl<'db, RTuple> Runs<'db> for RTuple
where
    RTuple: Transformation,
    RTuple::Data: Selection,
{
    fn run_on(&self, db: &EntityDatabase) -> TransformationResult {
        let mut row_components: Vec<ComponentType> = Vec::new();

        let reads = RTuple::Data::READS;
        let writes = RTuple::Data::WRITES;

        reads.iter().zip(writes.iter()).for_each(|(read, write)| {
            let component_access = read.or(*write).expect("expected read/write");
            row_components.push(component_access);
        });
        
        let component_set: ComponentTypeSet = ComponentTypeSet::from(row_components.clone());
        let matching_families: Vec<FamilyId> = db
            .query_mapping::<ComponentTypeSet, FamilyIdSet>(&component_set)
            .expect("expected established component family")
            .clone_into_vec();

        let mut column_keys: Vec<ColumnKey> = Vec::new();

        matching_families
            .iter()
            .map(|family| {
                db.get_table(family).expect("expected table")
            }).for_each(|table| {

                // iterating row_components here instead of component_set BECAUSE component sets are
                // ordered sets, whereas row_components is simply a vector with the same ordering
                // as the combination of our sparse read and write lists
                for component in row_components.iter() {
                    let key = table.column_map().get(component).expect("expected column key");
                    column_keys.push(*key);
                }
            });
        
        let rows = Rows::<RTuple::Data> {
            db,
            keys: column_keys,
            width: component_set.len(),
            marker: PhantomData,
        };
        
        RTuple::run(rows)
    }
}

struct DynTransform<'db> {
    ptr: Box<dyn Runs<'db>>,
    reads: Vec<ComponentType>,
    writes: Vec<ComponentType>,
}

impl<'db> Dependent for DynTransform<'db> {
    type Dependency = ComponentType;

    fn dependencies<'iter>(&'iter self)
    -> impl Iterator<Item = <DynTransform as Dependent>::Dependency> + 'iter {
        self.reads.iter().cloned()
    }

    fn exclusive_dependencies<'iter>(&'iter self)
    -> impl Iterator<Item = <DynTransform as Dependent>::Dependency> + 'iter {
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
    // allow statement fixes never read lint - this is in fact read but only from a macro
    #[allow(dead_code)] pub(crate) db: &'db EntityDatabase,

    marker: PhantomData<RTuple>,

    /// Full list of columns to iterate through, ordered by table,
    /// we track the keys, as well as the runtime checked borrows and pointers to
    /// the raw column data 
    pub(crate) keys: Vec<ColumnKey>,
    pub(crate) borrows: Vec<(RawBorrow, NonNull<c_void>)>,

    /// The width of the resultant tuple we yield during iteration
    pub(crate) width: usize,

    /// Which table we are currently iterating
    pub(crate) table_index: usize,

    /// Which column index we are currently yielding
    pub(crate) column_index: usize,
}

impl<'db, RTuple> RowIter<'db, RTuple> {
    pub fn new(db: &'db EntityDatabase) -> Self {
        Self {
            db,
            marker: Default::default(),
            keys: Default::default(),
            borrows: Default::default(),
            width: Default::default(),
            table_index: Default::default(),
            column_index: Default::default(),
        }
    }
}

pub struct Rows<'db, RTuple> {
    pub(crate) db: &'db EntityDatabase,
    pub(crate) keys: Vec<ColumnKey>,
    pub(crate) width: usize,
    pub(crate) marker: PhantomData<RTuple>,
}

impl<'db, RTuple> Rows<'db, RTuple> {
    pub fn database(&self) -> &'db EntityDatabase {
        self.db
    }
    
    pub fn keys(&self) -> &Vec<ColumnKey> {
        &self.keys
    }

    pub fn width(&self) -> usize {
        self.width
    }
}

#[const_trait]
pub trait SelectOne<'db> {
    type Ref;
    type Type;
    type BorrowType;

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
    type Ref = &'db Self::Type;
    type Type = C;
    type BorrowType = RawColumnRef<C>;

    fn reads() -> Option<ComponentType> {
        Some(ComponentType::of::<Self::Type>())
    }
}

impl<'db, C> const SelectOne<'db> for Write<C>
where
    Self: 'db,
    C: Component,
{
    type Ref = &'db mut Self::Type;
    type Type = C;
    type BorrowType = RawColumnRefMut<C>;

    fn writes() -> Option<ComponentType> {
        Some(ComponentType::of::<Self::Type>())
    }
}

#[const_trait]
pub trait Selection {
    const READS: &'static [Option<ComponentType>];
    const WRITES: &'static [Option<ComponentType>];
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
