#[allow(dead_code)] // during re-write only
#[macro_use]
pub mod reckoning {
    use core::fmt;
    // misc
    use std::any::Any;
    use std::error::Error;
    use std::fmt::Debug;
    use std::fmt::Display;
    use std::hash::Hash;

    // sync
    use std::sync::atomic::AtomicU32;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::PoisonError;
    use std::sync::RwLock;
    use std::sync::RwLockWriteGuard;

    // collections
    use std::collections::HashMap;
    
    // crate
    use crate::column::Column;
    use crate::column::ColumnHeader;
    use crate::column::ColumnInner;
    use crate::column::ColumnKey;
    use crate::column::ColumnMoveResult;
    use crate::column::ColumnSwapRemoveResult;
    use crate::components::Component;
    use crate::components::ComponentDelta;
    use crate::components::ComponentType;
    use crate::components::ComponentTypeSet;
    use crate::id::*;
    use crate::table::*;
    use crate::EntityId;

    // typedefs
    pub(crate) type AnyPtr = Box<dyn Any>;

    use dashmap::DashMap;
    use dashmap::try_result::TryResult as DashMapTryResult;
    use dashmap::mapref::one::Ref as DashMapRef;
    use dashmap::mapref::one::RefMut as DashMapRefMut;
    use transfer::TransferGraph;
    
    pub trait DbMapping<'db> {
        type Guard;
        type From;
        type Map;
        type To;
    }

    impl<'db, F, T> DbMapping<'db> for (F, T)
    where
        F: 'db,
        T: 'db + Clone,
    {
        type Guard = RwLockWriteGuard<'db, Self::Map>;
        type From = F;
        type Map = HashMap<F, T>;
        type To = T;
    }

    pub trait GetDbMap<'db, M: DbMapping<'db>> {
        fn get(&self, from: &M::From) -> Option<M::To>;
        fn mut_map(&'db self) -> Option<M::Guard>;
    }

    impl<'db> GetDbMap<'db, (ComponentTypeSet, FamilyId)> for DbMaps {
        fn get(
            &self,
            from: &<(ComponentTypeSet, FamilyId) as DbMapping>::From,
        ) -> Option<<(ComponentTypeSet, FamilyId) as DbMapping>::To> {
            self.component_group_to_family
                .read()
                .ok()
                .and_then(|g| g.get(from).cloned())
        }

        fn mut_map(&'db self) -> Option<<(ComponentTypeSet, FamilyId) as DbMapping>::Guard> {
            self.component_group_to_family.write().ok()
        }
    }

    impl<'db> GetDbMap<'db, (EntityId, FamilyId)> for DbMaps {
        fn get(
            &self,
            from: &<(EntityId, FamilyId) as DbMapping>::From,
        ) -> Option<<(EntityId, FamilyId) as DbMapping>::To> {
            self.entity_to_owning_family
                .read()
                .ok()
                .and_then(|g| g.get(from).cloned())
        }

        fn mut_map(&'db self) -> Option<<(EntityId, FamilyId) as DbMapping>::Guard> {
            self.entity_to_owning_family.write().ok()
        }
    }

    impl<'db> GetDbMap<'db, (ComponentType, FamilyIdSet)> for DbMaps {
        fn get(
            &self,
            from: &<(ComponentType, FamilyIdSet) as DbMapping>::From,
        ) -> Option<<(ComponentType, FamilyIdSet) as DbMapping>::To> {
            self.families_containing_component
                .read()
                .ok()
                .and_then(|g| g.get(from).cloned())
        }

        fn mut_map(&'db self) -> Option<<(ComponentType, FamilyIdSet) as DbMapping>::Guard> {
            self.families_containing_component.write().ok()
        }
    }

    impl<'db> GetDbMap<'db, (ComponentTypeSet, FamilyIdSet)> for DbMaps {
        fn get(
            &self,
            from: &<(ComponentTypeSet, FamilyIdSet) as DbMapping>::From,
        ) -> Option<<(ComponentTypeSet, FamilyIdSet) as DbMapping>::To> {
            self.families_containing_set
                .read()
                .ok()
                .and_then(|g| g.get(from).cloned())
        }

        fn mut_map(&'db self) -> Option<<(ComponentTypeSet, FamilyIdSet) as DbMapping>::Guard> {
            self.families_containing_set.write().ok()
        }
    }

    impl<'db> GetDbMap<'db, (FamilyId, ComponentTypeSet)> for DbMaps {
        fn get(
            &self,
            from: &<(FamilyId, ComponentTypeSet) as DbMapping>::From,
        ) -> Option<<(FamilyId, ComponentTypeSet) as DbMapping>::To> {
            self.components_of_family
                .read()
                .ok()
                .and_then(|g| g.get(from).cloned())
        }

        fn mut_map(&'db self) -> Option<<(FamilyId, ComponentTypeSet) as DbMapping>::Guard> {
            self.components_of_family.write().ok()
        }
    }

    impl<'db> GetDbMap<'db, (FamilyId, TransferGraph)> for DbMaps {
        fn get(
            &self,
            from: &<(FamilyId, ComponentTypeSet) as DbMapping>::From,
        ) -> Option<<(FamilyId, TransferGraph) as DbMapping>::To> {
            self.transfer_graph_of_family
                .read()
                .ok()
                .and_then(|g| g.get(from).cloned())
        }

        fn mut_map(&'db self) -> Option<<(FamilyId, TransferGraph) as DbMapping>::Guard> {
            self.transfer_graph_of_family.write().ok()
        }
    }

    /// Contains several maps used to cache relationships between
    /// data in the [EntityDatabase]. The data in [DbMaps] is
    /// intended to be cross-cutting, and of a higher order than
    /// data stored in components
    ///
    /// The tradeoff we're after here is to do a lot of work up-front when
    /// dealing with components and how they relate to one another. The
    /// strategy is to cache and record all of the informaiton about
    /// families at their creation, and then use that information to
    /// accelerate interactions with the [EntityDatabase]
    #[derive(Debug)]
    pub struct DbMaps {
        component_group_to_family: RwLock<HashMap<ComponentTypeSet, FamilyId>>,
        entity_to_owning_family: RwLock<HashMap<EntityId, FamilyId>>,
        families_containing_component: RwLock<HashMap<ComponentType, FamilyIdSet>>,
        families_containing_set: RwLock<HashMap<ComponentTypeSet, FamilyIdSet>>,
        components_of_family: RwLock<HashMap<FamilyId, ComponentTypeSet>>,
        transfer_graph_of_family: RwLock<HashMap<FamilyId, TransferGraph>>,
        // TODO: The traits that back DbMaps are designed to make
        // extensibility possible, dynamic mappings/extensions are
        // a future addition to be explored
    }

    impl<'db> DbMaps {
        fn new() -> Self {
            Self {
                component_group_to_family: Default::default(),
                entity_to_owning_family: Default::default(),
                families_containing_component: Default::default(),
                families_containing_set: Default::default(),
                components_of_family: Default::default(),
                transfer_graph_of_family: Default::default(),
            }
        }

        /// Retrieves a value from a mapping, if it exists
        ///
        /// Returned values are deliberately copied/cloned, rather than
        /// referenced. This is to avoid issues of long-lived references
        /// and so synchronization primitives are held for the shortest
        /// time possible. As such, it is encouraged to only map to small
        /// values, or, large values stored behind a reference counted
        /// pointer
        pub fn get_map<M>(&self, from: &M::From) -> Option<M::To>
        where
            M: DbMapping<'db>,
            Self: GetDbMap<'db, M>,
            M::To: 'db,
        {
            <Self as GetDbMap<'db, M>>::get(&self, from)
        }

        pub fn mut_map<M: DbMapping<'db>>(&'db self) -> Option<M::Guard>
        where
            M: DbMapping<'db>,
            M::Map: 'db,
            Self: GetDbMap<'db, M>,
        {
            <Self as GetDbMap<'db, M>>::mut_map(&self)
        }
    }

    

    pub struct Family {
        components_set: ComponentTypeSet,
        transfer_graph: transfer::TransferGraph,
    }

    impl Family {
        fn get_transfer(&self, _component: &ComponentType) -> Option<transfer::Edge> {
            todo!()
        }
    }

    #[derive(Debug)]
    enum EntityAllocError {
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

    type EntityFreeList = Vec<EntityId>;
    type EntityAllocResult = Result<EntityId, EntityAllocError>;

    impl<T> std::convert::From<PoisonError<T>> for EntityAllocError {
        fn from(_: PoisonError<T>) -> Self {
            EntityAllocError::PoisonedFreeList
        }
    }

    #[derive(Debug)]
    pub struct EntityAllocator {
        count: AtomicU32,
        free: Mutex<Vec<EntityId>>,
    }

    impl EntityAllocator {
        fn new() -> Self {
            Self {
                count: AtomicU32::new(0),
                free: Default::default(),
            }
        }

        fn alloc(&self) -> EntityAllocResult {
            // TODO: Better allocator: alloc blocks of ID's and cache them
            // per-thread, re-alloc a block when a thread runs out, reclaiming
            // the free list. ALternatively use a per thread free list

            let mut guard = self.free.lock()?;

            match guard.pop() {
                Some(id) => {
                    // SAFETY:
                    // Accessing union fields is implicitely unsafe - here we copy
                    // one union to another with the same accessor which is safe
                    let idunion = unsafe {
                        IdUnion {
                            generational: id.generational,
                        }
                    };
                    Ok(EntityId(idunion))
                }
                None => {
                    let count = self.count.fetch_add(1u32, Ordering::SeqCst);
                    Ok(EntityId(IdUnion {
                        generational: (count, 0, 0, 0),
                    }))
                }
            }
        }

        fn free(&self, id: EntityId) -> Result<(), EntityAllocError> {
            let mut guard = self.free.lock()?;
            guard.push(id.next_generation());
            Ok(())
        }
    }

    #[derive(Debug)]
    pub enum CreateEntityError {
        IdAllocatorError,
        DbError(DbError),
    }

    impl Display for CreateEntityError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                CreateEntityError::IdAllocatorError => write!(f, "entity id allocation error"),
                CreateEntityError::DbError(err) => {
                    write!(f, "database error while creating entity: {}", err)
                }
            }
        }
    }

    impl Error for CreateEntityError {}

    impl From<EntityAllocError> for CreateEntityError {
        fn from(_: EntityAllocError) -> Self {
            CreateEntityError::IdAllocatorError
        }
    }

    impl From<DbError> for CreateEntityError {
        fn from(err: DbError) -> Self {
            CreateEntityError::DbError(err)
        }
    }

    #[derive(Debug)]
    pub enum DbError {
        EntityDoesntExist(EntityId),
        FailedToResolveTransfer,
        FailedToFindEntityFamily(EntityId),
        FailedToFindFamilyForSet(ComponentTypeSet),
        EntityBelongsToUnknownFamily,
        FailedToAcquireMapping,
        ColumnTypeDiscrepancy,
        ColumnAccessOutOfBounds,
        TableDoesntExistForFamily(FamilyId),
        ColumnDoesntExistInTable,
        EntityNotInTable(EntityId, FamilyId),
        UnableToAcquireTablesLock(String),
        FamilyDoesntExist(FamilyId),
        UnableToAcquireLock,
        MoveWithSameColumn,
    }

    impl Error for DbError {}

    impl Display for DbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                DbError::EntityDoesntExist(entity) => {
                    write!(f, "entity {:?} doesn't exist", entity)
                }
                DbError::FailedToResolveTransfer => {
                    write!(f, "failed to transfer entity between families")
                }
                DbError::FailedToFindEntityFamily(entity) => {
                    write!(f, "failed to find a family for entity {:?}", entity)
                }
                DbError::FailedToFindFamilyForSet(set) => {
                    write!(
                        f,
                        "failed to find a family for the set of components {}",
                        set
                    )
                }
                DbError::EntityBelongsToUnknownFamily => {
                    write!(f, "requested family data is unknown or invalid")
                }
                DbError::FailedToAcquireMapping => {
                    write!(f, "failed to acquire requested mapping")
                }
                DbError::ColumnTypeDiscrepancy => {
                    write!(f, "column type mismatch")
                }
                DbError::ColumnAccessOutOfBounds => {
                    write!(f, "attempted to index a column out of bounds")
                }
                DbError::TableDoesntExistForFamily(family) => {
                    write!(f, "table doesn't exist for the given family id {}", family)
                }
                DbError::ColumnDoesntExistInTable => {
                    write!(f, "column doesn't exist in the given table")
                }
                DbError::EntityNotInTable(entity, family) => {
                    write!(f, "{:?} does not exist in {:?} data table", entity, family)
                }
                DbError::UnableToAcquireTablesLock(reason) => {
                    write!(f, "unable to acquire master table lock: {}", reason)
                }
                DbError::FamilyDoesntExist(family) => {
                    write!(f, "family doesn't exist: {}", family)
                }
                DbError::UnableToAcquireLock => {
                    write!(f, "failed to acquire poisoned lock")
                }
                DbError::MoveWithSameColumn => {
                    write!(f, "attempted to move a component to the column it was already in")
                }
            }
        }
    }

    #[derive(Debug)]
    pub struct EntityDatabase {
        allocator: EntityAllocator,

        /// Table structures describe who owns each column
        tables: Arc<DashMap<FamilyId, Table>>,

        /// The actual raw user data is stored in columns
        columns: Arc<DashMap<ColumnKey, Column>>,

        /// Data mappings and caches. Stores some critical information for
        /// quickly querying the DB
        maps: DbMaps,

        /// Cache of the column headers we've seen/created
        /// These can be used to quickly instantiate tables with
        /// types of columns we've already built
        headers: Arc<DashMap<ComponentType, ColumnHeader>>,
    }

    pub(crate) type ColumnMapRef<'db> = dashmap::mapref::one::Ref<'db, ColumnKey, Column>;
    pub(crate) type ColumnMapRefMut<'db> = dashmap::mapref::one::RefMut<'db, ColumnKey, Column>;

    pub(crate) type TableMapRef<'db> = dashmap::mapref::one::Ref<'db, FamilyId, Table>;

    impl EntityDatabase {
        /// Creates a new [EntityDatabase]
        pub fn new() -> Self {
            tracing::info!("constructing new entity database");

            let db = Self {
                allocator: EntityAllocator::new(),
                tables: Arc::new(DashMap::new()),
                columns: Arc::new(DashMap::new()),
                headers: Arc::new(DashMap::new()),
                maps: DbMaps::new(),
            };

            // prettier debug output when dealing with unit/null components
            StableTypeId::register_debug_info::<()>();

            // setup the unit/null component family
            let unit_family_set = ComponentTypeSet::from([ComponentType::of::<()>()]);
            let _ = db
                .new_family(unit_family_set)
                .expect("please report this bug - unable to create unit component family");
            db
        }

        pub(crate) fn get_column(&self, key: &ColumnKey) -> Option<ColumnMapRef> {
            tracing::trace!(%key, "IMMUTABLE COLUMN ACCESS");
            self.columns.get(key)
        }

        pub(crate) fn get_column_mut(&self, key: &ColumnKey) -> Option<ColumnMapRefMut> {
            tracing::trace!(%key, "⚠MUTABLE COLUMN ACCESS⚠");
            self.columns.get_mut(key)
        }

        pub(crate) fn insert_column(&self, key: ColumnKey, column: Column) {
            tracing::trace!(%key, "⚠MUTABLE COLUMN ACCESS⚠");
            self.columns.insert(key, column);
        }

        pub(crate) fn contains_column(&self, key: &ColumnKey) -> bool {
            self.columns.contains_key(key)
        }

        fn try_get_column(&self, key: &ColumnKey) -> DashMapTryResult<DashMapRef<ColumnKey, Column>> {
            return self.columns.try_get(key)
        }

        fn try_get_column_mut(&self, key: &ColumnKey) -> DashMapTryResult<DashMapRefMut<ColumnKey, Column>> {
            return self.columns.try_get_mut(key)
        }

        pub(crate) fn get_table(&self, family: &FamilyId) -> Option<TableMapRef> {
            self.tables.get(family)
        }
        
        pub fn update_mapping<'db, K: Debug + Clone + Eq + Hash + 'db, V: Debug + Clone + 'db>(
            &'db self,
            key: &'db K,
            value: &'db V,
        ) -> Result<(), DbError>
        where
            DbMaps: GetDbMap<'db, (K, V)>,
        {
            //tracing::trace!(
            //    k = ?key,
            //    v = ?value,
            //    "updating mapping"
            //);
            
            let mut guard = self
                .maps
                .mut_map::<(K, V)>()
                .ok_or(DbError::FailedToAcquireMapping)?;

            guard.insert(key.clone(), value.clone());
            Ok(())
        }

        pub fn delete_mapping<'db, K: Debug + Clone + Eq + Hash + 'db, V: Debug + Clone + 'db>(
            &'db self,
            key: &'db K,
        ) -> Result<(), DbError>
        where
            DbMaps: GetDbMap<'db, (K, V)>,
        {
            tracing::trace!(
                k = ?key,
                v_type = std::any::type_name::<V>(),
                "deleting mapping"
            );

            let mut guard = self
                .maps
                .mut_map::<(K, V)>()
                .ok_or(DbError::FailedToAcquireMapping)?;

            guard.remove(key);
            Ok(())
        }

        pub fn query_mapping<'db, K: Eq + Hash, V: Clone + 'db>(&'db self, key: &'db K) -> Option<V>
        where
            DbMaps: GetDbMap<'db, (K, V)>,
        {
            self.maps.get_map::<(K, V)>(key)
        }

        /// Creates an entity, returning its [EntityId]
        pub fn create(&self) -> Result<EntityId, CreateEntityError> {
            let _trace_span = tracing::span!(tracing::Level::DEBUG, "creating entity").entered();

            let entity = self
                .allocator
                .alloc()
                .map_err(|_| CreateEntityError::IdAllocatorError)?;

            let unit_family_set = ComponentTypeSet::from([ComponentType::of::<()>()]);
            let family: FamilyId =
                self.query_mapping(&unit_family_set)
                    .ok_or(CreateEntityError::DbError(
                        DbError::FailedToFindFamilyForSet(unit_family_set.clone()),
                    ))?;
            
            tracing::trace!(entity_id = ?entity, family_id = ?family);
            
            // add the entity to the unit/null family
            self.update_mapping(&entity, &family)?;
            self.initialize_row_in(&entity, &family)?;
            
            Ok(entity)
        }

        /// Destroys an entity, deleting its components and associated data
        pub fn destroy(&self, entity: EntityId) -> Result<(), DbError> {
            let family = self.query_mapping(&entity).ok_or(DbError::EntityDoesntExist(entity))?;
            let table = self
                .tables
                .get_mut(&family)
                .ok_or(DbError::TableDoesntExistForFamily(family))?;

            let index = *table
                .entity_map()
                .get(&entity)
                .ok_or(DbError::EntityNotInTable(entity, family))?;

            let mut swap_result: Option<ColumnSwapRemoveResult> = None;

            for (_, key) in table.column_map() {
                let mut column = self.get_column_mut(key)
                    .ok_or(DbError::ColumnDoesntExistInTable)?;
                
                // This should be the same for each column
                // If it's not, then this tables columns are corrupted
                if swap_result.is_some() {
                    let unwrapped = swap_result.as_ref().expect("expected swap result");
                    assert!(*unwrapped == column.swap_and_destroy(index));
                } else {
                    swap_result = Some(column.swap_and_destroy(index));
                }
            }

            if let Some(swap_result) = swap_result {
                let moved = swap_result.moved;
                let moved_index = swap_result.new_moved_index;

                table.entity_map().entry(moved).and_modify(|i| *i = moved_index);
            } else {
                // better error handling here plz
                panic!()
            }

            self.delete_mapping::<EntityId, FamilyId>(&entity)?;            
            Ok(())
        }


        
        fn build_typed_column_for_family(&self, family: FamilyId, ty: &ComponentType) -> Result<dashmap::mapref::one::RefMut<'_, ColumnKey, Column>, DbError> {
            let _trace_span = tracing::span!(tracing::Level::TRACE, "build_typed_column_for_family").entered();
            
            if let Some(header) = self.headers.get(ty) {
                let column_header = header.clone();
                let component_type = ComponentType::from(column_header.stable_type_id());
                let column_key = ColumnKey::from((family, component_type));
                let column_inner = (column_header.fn_constructor)();
                let column = Column::new(column_header.clone(), column_inner);
                
                { // guard scope
                    self.insert_column(column_key, column);
                }
                
                Ok(self.get_column_mut(&column_key).expect("expect just-created column"))
            } else {
                unimplemented!("lazy init columns?");
            }
        }

        

        /// Initializes the component data for a given `entity` belonging to a given `family`
        /// If the entity doesn't already exist in the table, this creates space for it
        /// 
        /// Returns the columnar index of the `entity`. Note, this index is never used to directly
        /// index any entity in user code, instead, if access to a specific entity is required then
        /// its associated [EntityId] must be used. The index returned here will change over time,
        /// it's useful only to update the [EntityDatabase]'s present state
        fn initialize_row_in(
            &self,
            entity: &EntityId,
            family: &FamilyId,
        ) -> Result<usize, DbError> {
            let _trace_span = tracing::span!(tracing::Level::TRACE, "initialize_row_in").entered();

            let mut table = self
                .tables
                .get_mut(family)
                .ok_or(DbError::TableDoesntExistForFamily(*family))?;
            
            match table.get_or_insert_entity(entity) {
                TableResult::EntityAlreadyExists(index) => {
                    return Ok(index)
                },
                TableResult::EntityInserted(index) => {
                    for (ty, key) in table.column_map() {
                        if !self.contains_column(key) {
                            let column_ref = self.build_typed_column_for_family(*family, ty)?;
                            
                            drop(column_ref)
                        } 
                    }
                    return Ok(index)
                },
                TableResult::Error(e) => {
                    return Err(e)
                },
            }
        }

        /// Adds a [Component] to an entity, moving it from one family to another
        ///
        /// If the newly create combination of components has never been created before,
        /// this will create a new family and its associated data columns
        pub fn add_component<C: Component>(
            &mut self,
            entity: EntityId,
            component: C,
        ) -> Result<(), DbError> {
            let _trace_span = tracing::span!(tracing::Level::DEBUG, "add_component").entered();
            tracing::trace!(entity_id = ?entity, component = ?component);

            StableTypeId::register_debug_info::<C>();

            let family = self
                .maps
                .get_map::<(EntityId, FamilyId)>(&entity)
                .ok_or(DbError::EntityDoesntExist(entity))?;

            let delta = ComponentDelta::Add(ComponentType::of::<C>());
            let new_family = self.find_new_family(&family, &delta)?;

            if !(family == new_family) {
                self.resolve_entity_transfer(&entity, &new_family)?;
            }

            self.set_component_for(&entity, component)?;
            
            Ok(())
        }

        /// Adds a single instance of a global component to the [EntityDatabase]
        /// A global component only ever has one instance, and is accessible by
        /// any system with standard Read/Write rules. Typical uses for a global
        /// component might be deferred events, or cross-cutting state such as
        /// input.
        pub fn add_global_component<T: Component>(&mut self) {
            // Consideration:
            //
            // Mutating global components should be particularly strict, perhaps
            // only in a specific phase at the beginning or end of the frame,
            // or at the end of each phase.
            //
            // Reading global components should be possible from any system at
            // any time

            todo!()
        }

        /// Retrieves the next [Command] generated by the [EntityDatabase], or
        /// returns [None] if there are no pending commands. Commands are used
        /// by the [EntityDatabase] to communicate with external control structures
        pub fn query_commands(&self) -> Option<Command> {
            // Notes:
            //
            // Still don't have a clear picture of how this should work
            // One thought is to have an implicit CommandQueue global component
            // which can be queried regularly by systems (perhaps with a special
            // Global<CommandQueue> accessor), from which the system can enqueue
            // higher order commands as if it were a regular component. However
            // this would introduce synchonization difficulties, unless commands
            // were collected per-thread and then the full stream was stitched
            // together at the end of each phase, or a wait-free queue was used

            None
        }

        /// Computes the destination family for a given entity, after
        /// a component addition or removal
        fn find_new_family(
            &self,
            family: &FamilyId,
            delta: &ComponentDelta,
        ) -> Result<FamilyId, DbError> {
            let _trace_span = tracing::span!(tracing::Level::TRACE, "find_new_family").entered();

            match delta {
                ComponentDelta::Add(component) => {
                    if let Some(components) = self.query_mapping::<FamilyId, ComponentTypeSet>(&family) {
                        if components.contains(component) {
                            return Ok(*family)
                        }
                    }

                    return self.family_after_add(&family, component);
                },
                ComponentDelta::Rem(component) => {
                    return self.family_after_remove(&family, component)
                },
            }
        }

        fn family_after_add(
            &self,
            curr_family: &FamilyId,
            new_component: &ComponentType,
        ) -> Result<FamilyId, DbError> {
            let _trace_span = tracing::span!(tracing::Level::TRACE, "family_after_add").entered();

            // First try and find an already cached edge on the transfer graph for this family
            if let Some(transfer::Edge::Add(family_id)) =
                self.query_transfer_graph(curr_family, new_component)
            {
                return Ok(family_id);
            } else {
                // Else resolve the family manually, and update the transfer graph
                let components: ComponentTypeSet = self
                    .query_mapping(curr_family)
                    .ok_or(DbError::EntityBelongsToUnknownFamily)?;

                if components.contains(&new_component) {
                    return Ok(*curr_family);
                } else {
                    let new_components_iter = components.iter().cloned().chain([*new_component]);
                    let new_components = ComponentTypeSet::from(new_components_iter);

                    let family = match self
                        .maps
                        .get_map::<(ComponentTypeSet, FamilyId)>(&new_components)
                    {
                        Some(family) => family,

                        // Create a new family for this unique set of components
                        None => self.new_family(new_components)?,
                    };


                    self.update_transfer_graph(
                        curr_family,
                        new_component,
                        transfer::Edge::Add(family),
                    )?;

                    self.update_transfer_graph(
                        &family,
                        new_component,
                        transfer::Edge::Remove(*curr_family),
                    )?;


                    Ok(family)
                }
            }
        }

        /// Computes or creates the resultant family after a component is
        /// removed from
        fn family_after_remove(
            &self,
            _: &FamilyId,
            _: &ComponentType,
        ) -> Result<FamilyId, DbError> {
            todo!()
        }

        /// Creates a new family, sets up the default db mappings for the family
        fn new_family(&self, components: ComponentTypeSet) -> Result<FamilyId, DbError> {
            let _trace_span = tracing::span!(tracing::Level::TRACE, "new_family").entered();


            let family_id = FamilyId::from_iter(components.iter());
            let mut headers: Vec<ColumnHeader> = Vec::with_capacity(components.len());

            components.iter().for_each(|ty| {
                if let Some(header) = self.headers.get(ty) {
                    headers.push(header.clone());
                } else {
                    // some other header init process?
                }
            });

            let mut table = Table::new(family_id, components.clone());
            {
                headers.iter().for_each(|header| {
                    let component_type = ComponentType::from(header.stable_type_id());
                    let column_inner = (header.fn_constructor)();
                    let column = Column::new(header.clone(), column_inner);
                    let column_key = ColumnKey::from((family_id, component_type));

                    tracing::trace!(%column_key, "creating new column");

                    self.insert_column(column_key, column);
                    table.update_column_map(component_type, column_key);
                });
            }

            self.tables.insert(family_id, table);

            self.update_mapping(&components, &family_id)?;
            self.update_mapping(&family_id, &components)?;
            self.update_mapping(&family_id, &TransferGraph::new())?;

            {
                let mut guard = self
                    .maps
                    .mut_map::<(ComponentType, FamilyIdSet)>()
                    .ok_or(DbError::FailedToAcquireMapping)?;

                // We map each component type to the set of families which contain it
                components.iter().for_each(|component_type| {
                    guard
                        .entry(*component_type)
                        .and_modify(|set| set.insert(family_id))
                        .or_insert(FamilyIdSet::from(&[family_id]));
                });
            }
            
            // Here we map every unique subset of our component set to this family
            // This results in 2^n unique mappings. These mappings are used by queries
            // to string together all of the columns necessary for row iteration.
            // In the future, we might want to intoduce a lazier mechanism for this,
            // or a way to trim unused mappings
            {
                let mut guard = self
                    .maps
                    .mut_map::<(ComponentTypeSet, FamilyIdSet)>()
                    .ok_or(DbError::FailedToAcquireMapping)?;

                for subset in components.power_set() {
                    guard
                        .entry(subset)
                        .and_modify(|set| set.insert(family_id))
                        .or_insert(FamilyIdSet::from(&[family_id]));
                }
            }

            Ok(family_id)
        }

        /// Tries to find an associated `TransferGraph` for the provided `FamilyId`, and then returns the
        /// appropriate transfer edge of that graph associated with the provided `ComponentType`. A `transfer::Edge`
        /// can be used to resolve either adding or removing a component from an entity
        fn query_transfer_graph(
            &self,
            family: &FamilyId,
            component: &ComponentType,
        ) -> Option<transfer::Edge> {
            self.maps
                .get_map::<(FamilyId, TransferGraph)>(family)
                .and_then(|graph| graph.get(component))
        }

        fn update_transfer_graph(
            &self,
            family: &FamilyId,
            component: &ComponentType,
            edge: transfer::Edge,
        ) -> Result<(), DbError> {
            let mut guard = self
                .maps
                .mut_map::<(FamilyId, TransferGraph)>()
                .ok_or(DbError::FailedToAcquireMapping)?;

            guard
                .get_mut(family)
                .and_then(|graph| graph.set(component, edge));

            Ok(())
        }

        fn remove_entity_from(
            &self,
            entity: &EntityId,
            family: &FamilyId,
        ) -> Result<(), DbError> {
            let mut table = self
                .tables
                .get_mut(family)
                .ok_or(DbError::TableDoesntExistForFamily(*family))?;

            table.remove_entity(entity)
        }

        fn move_components(
            &self,
            entity: &EntityId,
            from_family: &FamilyId,
            dest_family: &FamilyId,
        ) -> Result<(), DbError> {
            let _trace_span = tracing::span!(tracing::Level::TRACE, "move_components").entered();

            //let _trace_span = tracing::span!(tracing::Level::DEBUG, "moving components").entered();
            //tracing::trace!(entity_id = ?entity, from = ?from_family, dest = ?dest_family);

            let from_table = self
                .tables
                .get(from_family)
                .ok_or(DbError::TableDoesntExistForFamily(*from_family))?;

            let dest_table = self
                .tables
                .get(dest_family)
                .ok_or(DbError::TableDoesntExistForFamily(*dest_family))?;
            
            let from_index = *from_table
                .entity_map()
                .get(entity)
                .ok_or(DbError::EntityNotInTable(*entity, *from_family))?;

            tracing::trace!(index = from_index, "acquired table references + from table index");
            
            // assume the move will fail, update this if it succeeds
            let mut move_result: Result<ColumnMoveResult, DbError> = Err(DbError::ColumnDoesntExistInTable);
            
            for (ty, from_key) in from_table.column_map().iter() {
                let _tracing_span = tracing::span!(tracing::Level::TRACE, "moving component", ?ty).entered();

                let dest_key = match dest_table.column_map().get(ty) {
                    Some(key) => key,
                    None => { continue }
                };

                debug_assert_ne!(from_key, dest_key);
                
                let from_col = match self.try_get_column(from_key) {
                    DashMapTryResult::Present(column) => column,
                    DashMapTryResult::Absent => {
                        tracing::trace!(%from_key, %dest_key, "source column doesn't exist");
                        continue;
                    },
                    DashMapTryResult::Locked => {
                        tracing::trace!(%from_key, %dest_key, "source column already locked");
                        panic!();
                    },
                };

                let dest_col = match self.try_get_column(dest_key) {
                    DashMapTryResult::Present(column) => column,
                    DashMapTryResult::Absent => {
                        tracing::trace!(%from_key, %dest_key, "destination column doesn't exist");
                        continue;
                    },
                    DashMapTryResult::Locked => {
                        tracing::trace!(%from_key, %dest_key, "destination column already locked");
                        panic!();
                    },
                };

                let pending_move_result = from_col.move_component_to(&dest_col, from_index);
                tracing::trace!("pending move result: {:?}", pending_move_result);

                if let Some(e) = pending_move_result.as_ref().err() {
                    tracing::warn!("warning - {}", e);
                }

                if move_result.is_ok() && pending_move_result.is_err() {
                    tracing::error!("data column state out of sync");
                    panic!("data column state out of sync")
                } else {
                    move_result = pending_move_result;
                }
            }
            
            // if the move succeeded, patch the tables bookkeeping with the results of the move
            match move_result {
                Ok(ColumnMoveResult::SwapMoved { moved, new_moved_index, swapped, .. }) => {
                    tracing::trace!(?moved, ?new_moved_index, ?swapped, "swap move");

                    from_table.entity_map().insert(swapped, from_index);
                    from_table.entity_map().remove(&moved);
                    dest_table.entity_map().insert(moved, new_moved_index);

                    self.update_mapping(&moved, dest_family)?;
                    return Ok(())
                },
                Ok(ColumnMoveResult::Moved { moved, new_moved_index }) => {
                    tracing::trace!(?moved, ?new_moved_index, "moved");

                    dest_table.entity_map().insert(moved, new_moved_index);
                    from_table.entity_map().remove(&moved);
                    
                    self.update_mapping(&moved, dest_family)?;
                    return Ok(())
                },
                Ok(ColumnMoveResult::NoMove) => {
                    tracing::trace!("no move");

                    return Ok(())
                },
                Err(e) => {
                    return Err(e)
                },
            }
        }

        /// Transfers an entity out of its current family into the provided family, copying all component data
        ///
        /// This typically happens when a component is added or removed from the entity
        fn resolve_entity_transfer(
            &self,
            entity: &EntityId,
            dest_family: &FamilyId,
        ) -> Result<(), DbError> {
            let _trace_span = tracing::span!(tracing::Level::TRACE, "resolve_entity_transfer").entered();

            //let _trace_span = tracing::span!(tracing::Level::DEBUG, "resolving entity transfer").entered();
            //tracing::trace!(entity_id = ?entity, dest = ?dest_family);

            let from_family: FamilyId = self
                .query_mapping(entity)
                .ok_or(DbError::EntityDoesntExist(*entity))?;

            let _ = self.initialize_row_in(entity, dest_family)?;

            match self.move_components(entity, &from_family, dest_family) {
                Ok(_) => {},
                Err(DbError::ColumnDoesntExistInTable) => {
                    /* this can happen if the from family doesnt have any columns */
                },
                Err(e) => {
                    panic!("{:?}", e);
                }
            }

            self.update_mapping(entity, dest_family)?;
            self.remove_entity_from(entity, &from_family)
        }

        /// Explicitly sets a component for an entity. This is often the first time
        /// a real component of a given type is created in an entity/family, and thus
        /// we may need to actually initialize the data column in the table we are
        /// trying to set data for
        fn set_component_for<C: Component>(
            &self,
            entity: &EntityId,
            component: C,
        ) -> Result<(), DbError> {
            let _trace_span = tracing::span!(tracing::Level::TRACE, "set_component_for").entered();


            let family: FamilyId = self
                .query_mapping(entity)
                .ok_or(DbError::EntityDoesntExist(*entity))?;

            // here we get the table that the entity should be in
            // we error out if the table doesn't exist for some reason
            let table = self
                .tables
                .get(&family)
                .ok_or(DbError::TableDoesntExistForFamily(family))?;

            // here we get the index that the table is storing the entity in
            // we error out the table doesn't know about the entity
            let index = *table
                .entity_map()
                .get(entity)
                .ok_or(DbError::EntityNotInTable(*entity, family))?;

            // here we get the column key for the specific component we're
            // interested in. column keys are a combination of the component type,
            // and the table it belongs to
            let opt_has_column_key = table.column_map().get(&ComponentType::of::<C>()).cloned();

            drop(table); // release read lock. we might re-acquire a write lock below

            // here we check if the column we are interested actually exists or not
            // if it doesn't exist, we create it first
            // if/when it does exist, we instantiate space within it for our entity
            match opt_has_column_key {
                Some(column_key) => {
                    let _trace_span = tracing::span!(tracing::Level::TRACE, "opt_has_column_key:SOME").entered();

                    self.get_column_mut(&column_key)
                        .ok_or(DbError::ColumnDoesntExistInTable)?
                        .set_component(entity, index, component)?;
                },

                None => {
                    let _trace_span = tracing::span!(tracing::Level::TRACE, "opt_has_column_key:NONE").entered();

                    let mut table = self
                        .tables
                        .get_mut(&family)
                        .ok_or(DbError::TableDoesntExistForFamily(family))?;

                    let component_type = ComponentType::of::<C>();
                    let new_column_key = ColumnKey::from((family, component_type));

                    let header = ColumnHeader {
                        tyid: component_type.inner(),
                        fn_constructor: ColumnInner::<C>::dynamic_ctor,
                        fn_instance: ColumnInner::<C>::dynamic_push_instance,
                        fn_move: ColumnInner::<C>::dynamic_move,
                        fn_swap_and_destroy: ColumnInner::<C>::dynamic_swap_and_destroy,
                        fn_debug: ColumnInner::<C>::dynamic_debug,
                    };

                    let column_inner = (header.fn_constructor)();
                    let mut column = Column::new(header.clone(), column_inner);

                    let instance_index = column.instance_with(entity, component)?;

                    self.columns.insert(new_column_key, column);
                    self.headers.insert(component_type, header.clone());

                    table.update_column_map(component_type, new_column_key);
                    table.entity_map().insert(*entity, instance_index);
                }
            }

            Ok(())
        }
    }

    impl Display for EntityDatabase {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "DATABASE DUMP...\n")?;
            {
                for item in self.tables.iter() {
                    if item.entity_map().is_empty() {
                        continue;
                    }

                    write!(f, "{}\n", *item)?;
                    for (ty, key) in item.column_map().iter() {
                        match self.get_column(key) {
                            Some(column) => {
                                write!(f, "{}", *column)?;
                            }
                            None => {
                                write!(f, "[Unable to fetch column] ({})", ty)?;
                            }
                        }
                    }

                    write!(f, "\n\n\n\n")?;
                }
            }
            write!(f, "\n\n\n\n")
        }
    }

    /// A [Command] generated by an [EntityDatabase]
    /// [Command]'s are retrieved one at a time via the query_commands() method
    /// on an [EntityDatabase]
    pub enum Command {
        Quit,
    }

    /// Transfer
    ///
    /// Functionality related to quickly transferring entities from one
    /// family to another within an [super::EntityDatabase]
    mod transfer {
        use super::FamilyId;
        use crate::database::reckoning::ComponentType;
        use std::collections::HashMap;
        use std::sync::Arc;
        use std::sync::RwLock;

        #[derive(Debug, Clone)]
        pub struct TransferGraph {
            links: Arc<RwLock<HashMap<ComponentType, Edge>>>,
        }

        impl TransferGraph {
            pub fn new() -> Self {
                Self {
                    links: Arc::new(RwLock::new(HashMap::new())),
                }
            }

            pub fn get(&self, component: &ComponentType) -> Option<Edge> {
                match self.links.read() {
                    Ok(graph) => graph.get(component).cloned(),
                    Err(e) => {
                        panic!("unable to read transfer graph - rwlock - {:?}", e)
                    }
                }
            }

            pub fn set(&self, component: &ComponentType, edge: Edge) -> Option<Edge> {
                match self.links.write() {
                    Ok(mut graph) => graph.insert(*component, edge),
                    Err(e) => {
                        panic!("unable to set transfer graph - rwlock - {:?}", e)
                    }
                }
            }
        }

        #[derive(Debug, Clone)]
        pub enum Edge {
            Add(FamilyId),
            Remove(FamilyId),
        }
    } // transfer ======================================================================

    pub trait GetAsRefType<'db, S: crate::transform::SelectOne<'db>, R> {
        unsafe fn get_as_ref_type(&self, index: usize) -> Option<R>;
    }
    
    // Type system gymnastics
    impl<'db, S: crate::transform::SelectOne<'db>> GetAsRefType<'db, S, &'db S::Type> for *mut Vec<<S as crate::transform::SelectOne<'db>>::Type>
    {
        unsafe fn get_as_ref_type(&self, index: usize) -> Option<&'db S::Type> {
            (**self).get(index)
        }
    }

    impl<'db, S: crate::transform::SelectOne<'db>> GetAsRefType<'db, S, &'db mut S::Type> for *mut Vec<<S as crate::transform::SelectOne<'db>>::Type>
    {
        unsafe fn get_as_ref_type(&self, index: usize) -> Option<&'db mut S::Type> {
            (**self).get_mut(index)
        }
    }
    
    /// Macros
    ///
    /// WARNING: TYPE SYSTEM/MACRO SHENANIGANS BEYOND THIS POINT
    /// 
    /// These macros make it possible to perform ergonomic
    /// selections of data in an [super::EntityDatabase]
    #[macro_use]
    pub mod macros {
        #[macro_export]
        macro_rules! impl_transformations {
            ($([$t:ident, $i:tt]),*) => {
                #[allow(unused_parens)]
                impl<'db, $($t),+> Iterator for crate::transform::RowIter<'db, ($($t),+)>
                where
                    $(
                        $t: crate::transform::MetaData,
                        $t: crate::transform::SelectOne<'db>,
                        <$t as crate::transform::SelectOne<'db>>::Type: crate::components::Component,
                        *mut Vec<$t::Type>: crate::database::reckoning::GetAsRefType<'db, $t, <$t as crate::transform::SelectOne<'db>>::Ref>,
                    )+
                {
                    type Item = ($($t::Ref,)+);

                    fn next(&mut self) -> Option<Self::Item> {
                        if self.table_index >= (self.keys.len() / self.width) {
                            return None;
                        }
                        
                        use crate::database::reckoning::GetAsRefType;
                        let row: Self::Item = (
                            $(
                                unsafe {
                                    // Here we take a reference to our borrow and an opaque pointer to the column we're interested in
                                    // and through various type system gymnastics we transform the pointer into an accessor with the
                                    // correct mutablity, try to get a component from the column, and check for out of bounds
                                    let (_, pointer): &(crate::borrowed::RawBorrow, std::ptr::NonNull<std::os::raw::c_void>) = self.borrows.get_unchecked(self.table_index + $i);
                                    let casted: std::ptr::NonNull<Vec<$t::Type>> = pointer.cast::<Vec<$t::Type>>();
                                    let raw: *mut Vec<$t::Type> = casted.as_ptr();
                                    if let Some(result) = <*mut Vec<$t::Type> as GetAsRefType<'db, $t, <$t as crate::transform::SelectOne<'db>>::Ref>>::get_as_ref_type(&raw, self.column_index) {
                                        result
                                    } else {
                                        self.table_index += 1;
                                        self.column_index = 0;
                                        return self.next()
                                    }
                                }
                            ,)+
                        );
                        self.column_index += 1;
                        Some(row)
                    }
                }
                
                #[allow(unused_parens)]
                impl<'db, $($t),+> IntoIterator for crate::transform::Rows<'db, ($($t),+)>
                where
                    $(
                        $t: crate::transform::MetaData,
                        $t: crate::transform::SelectOne<'db>,
                        <$t as crate::transform::SelectOne<'db>>::Type: crate::components::Component,
                        <$t as crate::transform::SelectOne<'db>>::BorrowType: crate::column::BorrowAsRawParts,
                        crate::column::Column: crate::column::BorrowColumnAs<<$t as crate::transform::SelectOne<'db>>::Type, <$t as crate::transform::SelectOne<'db>>::BorrowType>,
                        *mut Vec<$t::Type>: crate::database::reckoning::GetAsRefType<'db, $t, <$t as crate::transform::SelectOne<'db>>::Ref>,
                        $t: 'static,
                    )+
                {
                    type Item = ($($t::Ref,)+);
                    type IntoIter = crate::transform::RowIter<'db, ($($t),+)>;

                    fn into_iter(self) -> Self::IntoIter {
                        (&self).into_iter()
                    }
                }

                #[allow(unused_parens)]
                impl<'db, $($t),+> IntoIterator for &crate::transform::Rows<'db, ($($t),+)>
                where
                    $(
                        $t: crate::transform::MetaData,
                        $t: crate::transform::SelectOne<'db>,
                        <$t as crate::transform::SelectOne<'db>>::Type: crate::components::Component,
                        <$t as crate::transform::SelectOne<'db>>::BorrowType: crate::column::BorrowAsRawParts,
                        *mut Vec<$t::Type>: crate::database::reckoning::GetAsRefType<'db, $t, <$t as crate::transform::SelectOne<'db>>::Ref>,
                        crate::column::Column: crate::column::BorrowColumnAs<<$t as crate::transform::SelectOne<'db>>::Type, <$t as crate::transform::SelectOne<'db>>::BorrowType>,
                        $t: 'static,
                    )+
                {
                    type Item = ($($t::Ref,)+);
                    type IntoIter = crate::transform::RowIter<'db, ($($t),+)>;

                    fn into_iter(self) -> Self::IntoIter {
                        let db = self.database();
                        let mut iter = crate::transform::RowIter::<'db, ($($t),+)>::new(db);
                        
                        for i in 0..(self.keys.len() / self.width) {
                            let borrows: ($($t::BorrowType,)+) = ($(
                                unsafe {
                                    use crate::column::BorrowColumnAs;
                                    let col_idx = (i * self.width) + $i;
                                    let column = db.get_column(self.keys.get_unchecked(col_idx)).expect("expected initialized column for iteration");
                                    <$t as crate::transform::SelectOne<'db>>::BorrowType::from(column.borrow_column_as())
                                }
                            ,)+);
                            $(
                                unsafe {
                                    use crate::column::BorrowAsRawParts;
                                    iter.borrows.push((borrows.$i).borrow_as_raw_parts());
                                }
                            )+
                        }
                        iter.keys = self.keys.clone();
                        iter.width = self.width;
                        iter
                    }
                }
                
                #[allow(unused_parens)]
                impl<'a, $($t),+> const crate::transform::Selection for ($($t),+)
                where
                    $(
                        $t: crate::transform::MetaData,
                        $t: ~const crate::transform::SelectOne<'a>,
                    )+
                {
                    const READS: &'static [Option<crate::components::ComponentType>] = &[$($t::reads(),)+];
                    const WRITES: &'static [Option<crate::components::ComponentType>] = &[$($t::writes(),)+];
                }
            };
        }
    } // macros ========================================================================
} // reckoning =========================================================================

// Exports
pub use crate::database::reckoning::EntityDatabase;

// Macro Impl's
impl_transformations!([A, 0]);
impl_transformations!([A, 0], [B, 1]);
impl_transformations!([A, 0], [B, 1], [C, 2]);
impl_transformations!([A, 0], [B, 1], [C, 2], [D, 3]);
impl_transformations!([A, 0], [B, 1], [C, 2], [D, 3], [E, 4]);
impl_transformations!([A, 0], [B, 1], [C, 2], [D, 3], [E, 4], [F, 5]);
impl_transformations!([A, 0], [B, 1], [C, 2], [D, 3], [E, 4], [F, 5], [G, 6]);
impl_transformations!([A, 0], [B, 1], [C, 2], [D, 3], [E, 4], [F, 5], [G, 6], [H, 7]);
