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
    use std::sync::Arc;
    use std::sync::RwLockReadGuard;
    use std::sync::RwLockWriteGuard;
    use std::sync::RwLock;
    use std::sync::PoisonError;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicU32;
    use std::sync::atomic::Ordering;
    
    // collections
    use std::collections::BTreeSet;
    use std::collections::HashMap;

    // crate
    use crate::EntityId;
    use crate::column::Column;
    use crate::column::ColumnKey;
    use crate::id::*;
    use crate::table::*;

    // typedefs
    pub(crate) type CType = self::ComponentType;
    pub(crate) type CTypeSet = self::ComponentTypeSet;
    pub(crate) type ColumnReadGuard<'a> = RwLockReadGuard<'a, HashMap<CType, TableEntry>>;
    pub(crate) type ColumnWriteGuard<'a> = RwLockWriteGuard<'a, HashMap<CType, TableEntry>>;
    pub(crate) type AnyPtr = Box<dyn Any>;

    use dashmap::DashMap;
    use transfer::TransferGraph;

    pub trait Component: Default + Debug + 'static {}
    
    /// Component implementation for the unit type
    /// Every entity automatically gets this component upon creation
    impl Component for () {}

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
        fn get(&self, from: &<(ComponentTypeSet, FamilyId) as DbMapping>::From) -> Option<<(ComponentTypeSet, FamilyId) as DbMapping>::To> {
            self.component_group_to_family.read().ok().and_then(|g| g.get(from).cloned())
        }

        fn mut_map(&'db self) -> Option<<(ComponentTypeSet, FamilyId) as DbMapping>::Guard> {
            self.component_group_to_family.write().ok()
        }
    }

    impl<'db> GetDbMap<'db, (EntityId, FamilyId)> for DbMaps {
        fn get(&self, from: &<(EntityId, FamilyId) as DbMapping>::From) -> Option<<(EntityId, FamilyId) as DbMapping>::To> {
            self.entity_to_owning_family.read().ok().and_then(|g| g.get(from).cloned())
        }

        fn mut_map(&'db self) -> Option<<(EntityId, FamilyId) as DbMapping>::Guard> {
            self.entity_to_owning_family.write().ok()
        }
    }

    impl<'db> GetDbMap<'db, (CType, FamilyIdSet)> for DbMaps {
        fn get(&self, from: &<(CType, FamilyIdSet) as DbMapping>::From) -> Option<<(CType, FamilyIdSet) as DbMapping>::To> {
            self.families_containing_component.read().ok().and_then(|g| g.get(from).cloned())
        }

        fn mut_map(&'db self) -> Option<<(CType, FamilyIdSet) as DbMapping>::Guard> {
            self.families_containing_component.write().ok()
        }
    }

    impl<'db> GetDbMap<'db, (CTypeSet, FamilyIdSet)> for DbMaps {
        fn get(&self, from: &<(CTypeSet, FamilyIdSet) as DbMapping>::From) -> Option<<(CTypeSet, FamilyIdSet) as DbMapping>::To> {
            self.families_containing_set.read().ok().and_then(|g| g.get(from).cloned())
        }

        fn mut_map(&'db self) -> Option<<(CTypeSet, FamilyIdSet) as DbMapping>::Guard> {
            self.families_containing_set.write().ok()
        }
    }

    impl<'db> GetDbMap<'db, (FamilyId, CTypeSet)> for DbMaps {
        fn get(&self, from: &<(FamilyId, CTypeSet) as DbMapping>::From) -> Option<<(FamilyId, CTypeSet) as DbMapping>::To> {
            self.components_of_family.read().ok().and_then(|g| g.get(from).cloned())
        }

        fn mut_map(&'db self) -> Option<<(FamilyId, CTypeSet) as DbMapping>::Guard> {
            self.components_of_family.write().ok()
        }
    }

    impl<'db> GetDbMap<'db, (FamilyId, TransferGraph)> for DbMaps {
        fn get(&self, from: &<(FamilyId, CTypeSet) as DbMapping>::From) -> Option<<(FamilyId, TransferGraph) as DbMapping>::To> {
            self.transfer_graph_of_family.read().ok().and_then(|g| g.get(from).cloned())
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
    pub struct DbMaps {
        component_group_to_family:      RwLock<HashMap<CTypeSet, FamilyId>>,
        entity_to_owning_family:        RwLock<HashMap<EntityId, FamilyId>>,
        families_containing_component:  RwLock<HashMap<CType,    FamilyIdSet>>,
        families_containing_set:        RwLock<HashMap<CTypeSet, FamilyIdSet>>,
        components_of_family:           RwLock<HashMap<FamilyId, CTypeSet>>,
        transfer_graph_of_family:       RwLock<HashMap<FamilyId, TransferGraph>>,

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
        pub fn get<M>(&self, from: &M::From) -> Option<M::To>
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
    
    #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct ComponentType(StableTypeId);

    impl ComponentType {
        pub const fn of<C: Component>() -> Self {
            Self(StableTypeId::of::<C>())
        }

        pub fn inner(&self) -> StableTypeId {
            return self.0
        }
    }

    impl Display for ComponentType {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0.name().unwrap_or("{unknown}"))
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct ComponentTypeSet {
        ptr: Arc<BTreeSet<CType>>, // thread-local?
        id: u64,
    }

    impl ComponentTypeSet {
        fn contains(&self, component: &ComponentType) -> bool {
            self.ptr.contains(component)
        }

        fn iter(&self) -> impl Iterator<Item = &CType> {
            self.ptr.iter()
        }

        fn names(&self) -> String {
            let out = self.ptr.iter().fold(String::new(), |out, c| out + &String::from(format!("{}, ", c)));
            format!("[{}]", out.trim_end_matches([' ', ',']))
        }
    }

    impl Display for ComponentTypeSet {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            for ty in self.ptr.iter() {
                write!(f, "{:?}", ty)?;
            }
            Ok(())
        }
    }

    impl FromIterator<ComponentType> for ComponentTypeSet {
        fn from_iter<T: IntoIterator<Item = ComponentType>>(iter: T) -> Self {
            let mut id: u64 = 0;
            
            // sum the unique 64 bit id's for each component type
            // and collect them into a set, use the summed id
            // (which should have a similar likelyhood of collision
            // as any 64 bit hash) and use that as the unique id for
            // the set of components
            let set: BTreeSet<ComponentType> = iter
                .into_iter()
                .map(|c| {id = id.wrapping_add(c.0.0); c})
                .collect();
            ComponentTypeSet { ptr: Arc::new(set), id }
        }
    }

    #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    enum ComponentDelta {
        Add(ComponentType),
        Rem(ComponentType),
    }

    pub struct Family {
        components_set: CTypeSet,
        transfer_graph: transfer::TransferGraph,
    }

    impl Family {
        fn get_transfer(&self, _component: &CType) -> Option<transfer::Edge> { todo!() }
    }

    #[derive(Debug)]
    enum EntityAllocError {
        PoisonedFreeList
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
                    let idunion = unsafe { IdUnion { generational: id.generational } };
                    Ok(EntityId(idunion))
                },
                None => {
                    let count = self.count.fetch_add(1u32, Ordering::SeqCst);
                    Ok(EntityId(IdUnion { generational: (count, 0, 0, 0) }))
                },
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
                CreateEntityError::DbError(err) => write!(f, "database error while creating entity: {}", err),
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
    }

    impl Error for DbError {}

    impl Display for DbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                DbError::EntityDoesntExist(entity) => {
                    write!(f, "entity {:?} doesn't exist", entity)
                },
                DbError::FailedToResolveTransfer => {
                    write!(f, "failed to transfer entity between families")
                },
                DbError::FailedToFindEntityFamily(entity) => {
                    write!(f, "failed to find a family for entity {:?}", entity)
                },
                DbError::FailedToFindFamilyForSet(set) => {
                    write!(f, "failed to find a family for the set of components {}", set)
                },
                DbError::EntityBelongsToUnknownFamily => {
                    write!(f, "requested family data is unknown or invalid")
                },
                DbError::FailedToAcquireMapping => {
                    write!(f, "failed to acquire requested mapping")
                },
                DbError::ColumnTypeDiscrepancy => {
                    write!(f, "column type mismatch")
                },
                DbError::ColumnAccessOutOfBounds => {
                    write!(f, "attempted to index a column out of bounds")
                },
                DbError::TableDoesntExistForFamily(family) => {
                    write!(f, "table doesn't exist for the given family id {}", family)
                },
                DbError::ColumnDoesntExistInTable => {
                    write!(f, "column doesn't exist in the given table")
                },
                DbError::EntityNotInTable(entity, family) => {
                    write!(f, "{:?} does not exist in {:?} data table", entity, family)
                },
                DbError::UnableToAcquireTablesLock(reason) => {
                    write!(f, "unable to acquire master table lock: {}", reason)
                },
                DbError::FamilyDoesntExist(family) => {
                    write!(f, "family doesn't exist: {}", family)
                },
                DbError::UnableToAcquireLock => {
                    write!(f, "failed to acquire poisoned lock")
                }
            }
        }
    }
    
    pub struct EntityDatabase {
        allocator: EntityAllocator,

        /// Table structures describe who owns each column
        tables: Arc<DashMap<FamilyId, Table>>,
        
        /// The actual raw user data is stored in columns
        columns: Arc<DashMap<ColumnKey, Column>>, 
        
        /// Data mappings and caches. Stores some critical information for
        /// quickly querying the DB
        maps: DbMaps, // separate cache?
    }
    
    impl EntityDatabase {
        /// Creates a new [EntityDatabase]
        pub fn new() -> Self {
            let db = Self {
                allocator: EntityAllocator::new(),
                tables: Arc::new(DashMap::new()),
                columns: Arc::new(DashMap::new()),
                maps: DbMaps::new(),
            };

            // prettier debug output when dealing with unit/null components
            StableTypeId::register_debug_info::<()>();

            // setup the unit/null component family
            let unit_family_set = ComponentTypeSet::from_iter([ComponentType::of::<()>()]);
            let family_id = db.new_family(unit_family_set).expect("please report this bug - unable to create unit component family");
            db
        }
        
        pub fn update_mapping<'db, K: Eq + Hash, V: Clone>(&'db self, key: &'db K, value: &'db V) -> Result<(), DbError>
        where
            //(K, V): GetDbMap<'db, (K, V)>,
            DbMaps: GetDbMap<'db, (K, V)>,
        {
            let mut guard = self.maps
                .mut_map::<(K, V)>()
                .ok_or(DbError::FailedToAcquireMapping)?;

            guard.insert(*key, *value);
            Ok(())
        }

        pub fn query_mapping<'db, K: Eq + Hash, V: Clone>(&'db self, key: &'db K) -> Option<&'db V>
        where
            DbMaps: GetDbMap<'db, (K, V)>,
        {
            self.maps.get::<(K, V)>(key).as_ref()
        }
        
        /// Creates an entity, returning its [EntityId]
        pub fn create(&self) -> Result<EntityId, CreateEntityError> {
            let entity = self.allocator
                .alloc()
                .map_err(|_| {CreateEntityError::IdAllocatorError})?;

            let unit_family_set = ComponentTypeSet::from_iter([ComponentType::of::<()>()]);
            let family: &FamilyId = self
                .query_mapping(&unit_family_set)
                .ok_or(CreateEntityError::DbError(DbError::FailedToFindFamilyForSet(unit_family_set)))?;
            
            // add the entity to the unit/null family
            self.update_mapping(&entity, &family);
            self.create_entity(&entity, &family)?;
            Ok(entity)
        }

        fn create_entity(&self, entity: &EntityId, family: &FamilyId) -> Result<(), DbError> {
            let mut table = self.tables.get(family)
                .ok_or(DbError::TableDoesntExistForFamily(*family))?;

            let index = table.insert_new_entity(entity)?;
            
            for (ty, key) in table.column_map() {
                if let Some(column) = self.columns.get(key) {
                    
                }
            }
            

            Ok(())
        }
        
        /// Adds a [Component] to an entity
        pub fn add_component<C: Component>(
            &mut self,
            entity: EntityId,
            component: C
        ) -> Result<(), DbError>
        {
            StableTypeId::register_debug_info::<C>(); // TODO: Fine to do this multiple times, but has perf impact, do once

            let cty = ComponentType::of::<C>();
            let delta = ComponentDelta::Add(cty);

            let cur_family = self.maps.get::<(EntityId, FamilyId)>(&entity);
            let new_family = self.find_new_family(&entity, &delta)?;
            
            self.resolve_entity_transfer(&entity, &new_family)?;
            self.set_component_for(&entity, component)?;
            
            Ok(())
        }
        
        /// Adds a single instance of a global component to the [EntityDatabase]
        /// A global component only ever has one instance, and is accessible by
        /// any system with standard Read/Write rules. Typical uses for a global
        /// component might be deferred events, or cross-cutting state such as
        /// input
        pub fn add_global_component<T: Component>(&mut self) {
            todo!()
        }

        /// Retrieves the next [Command] generated by the [EntityDatabase], or
        /// returns [None] if there are no pending commands. Commands are used
        /// by the [EntityDatabase] to communicate with the main game loop
        pub fn query_commands(&self) -> Option<Command> {
            // IMPL DETAILS
            // Still don't have a clear picture of how this should work
            // One thought is to have an implicit CommandQueue global component
            // which can be queried regularly by systems (perhaps with a special
            // Global<CommandQueue> accessor), from which the system can enqueue
            // higher order commands as if it were a regular component
            
            todo!()
        }

        /// Computes the destination family for a given entity, after
        /// a component addition or removal
        fn find_new_family(
            &self,
            entity: &EntityId,
            delta: &ComponentDelta
        ) -> Result<FamilyId, DbError> {
            let family = self.maps
                .get::<(EntityId, FamilyId)>(entity)
                .ok_or(DbError::EntityDoesntExist(*entity))?;

            match delta {
                ComponentDelta::Add(component) => {
                    self.family_after_add(&family, component)
                },
                ComponentDelta::Rem(component) => {
                    self.family_after_remove(&family, component)
                },
            }
        }

        fn family_after_add(
            &self,
            current: &FamilyId,
            component: &ComponentType
        ) -> Result<FamilyId, DbError> {
            
            // First try and find an already cached edge on the transfer graph for this family
            if let Some(transfer::Edge::Add(family_id)) = self.query_transfer_graph(current, component) {
                return Ok(family_id)
            } else {
                // Else resolve the family manually, and update the transfer graph
                let components = self.maps
                    .get::<(FamilyId, ComponentTypeSet)>(current)
                    .ok_or(DbError::EntityBelongsToUnknownFamily)?;

                if components.contains(&component) {
                    return Ok(*current)
                } else {
                    let new_components_iter =
                        components
                        .iter()
                        .cloned()
                        .chain([*component]);

                    let new_components = ComponentTypeSet::from_iter(new_components_iter);

                    let family = match self.maps.get::<(ComponentTypeSet, FamilyId)>(&new_components) {
                        Some(family) => {
                            family
                        },
                        None => {
                            self.new_family(new_components)?
                        }
                    };
                    
                    self.update_transfer_graph(current, component, transfer::Edge::Add(family))?;
                    self.update_transfer_graph(&family, component, transfer::Edge::Remove(*current))?;

                    Ok(family)
                }
            }
        }

        /// Computes or creates the resultant family after a component is
        /// removed from 
        fn family_after_remove(
            &self,
            _: &FamilyId,
            _: &ComponentType
        ) -> Result<FamilyId, DbError> {
            todo!()
        }

        /// Creates a new family, sets up the default db mappings for the family
        fn new_family(
            &self,
            components: ComponentTypeSet,
        ) -> Result<FamilyId, DbError> {
            println!("CREATING NEW FAMILY");

            let family_id = FamilyId::from_iter(components.iter());
            {
                if let Some(base_table) = self.tables.get_mut(&family_id) {
                    println!("USING BASE TABLE");
                    
                    let cloned_table = base_table.clone_new(family_id);
                    
                    {
                        let mut cloned_table_columns_guard = cloned_table.columns()
                            .write()
                            .expect("unable to lock cloned table columns for write");
                        
                        for (tyid, table_entry) in cloned_table_columns_guard.iter_mut() {
                            table_entry.data = (table_entry.fn_constructor)()
                        }
                    }

                    self.tables.insert(family_id, cloned_table);
                } else {
                    self.tables.insert(family_id, Table::new(family_id));
                }
            }
            
            // We map each family to the set of components it represents
            {
                let mut guard = self.maps
                    .mut_map::<(FamilyId, CTypeSet)>()
                    .ok_or(DbError::FailedToAcquireMapping)?;
                guard.insert(family_id, components.clone());
            }

            // We reverse map each set of components to its family id
            {
                let mut guard = self.maps
                    .mut_map::<(CTypeSet, FamilyId)>()
                    .ok_or(DbError::FailedToAcquireMapping)?;
                guard.insert(components.clone(), family_id);
            }

            // We map each family to an associated transfer graph
            {
                let mut guard = self.maps
                    .mut_map::<(FamilyId, TransferGraph)>()
                    .ok_or(DbError::FailedToAcquireMapping)?;
                guard.insert(family_id, TransferGraph::new());
            }

            // We map each component type in the set to every family that contains it
            {
                let mut guard = self.maps
                    .mut_map::<(CType, FamilyIdSet)>()
                    .ok_or(DbError::FailedToAcquireMapping)?;

                for cty in components.iter() {
                    let set;

                    match guard.remove(cty) {
                        Some(old_family_set) => {
                            set = FamilyIdSet::from(old_family_set.iter().chain([&family_id]));
                        },
                        None => {
                            set = FamilyIdSet::from(&[family_id]);
                        },
                    }
                    guard.insert(*cty, set);
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
            component: &ComponentType
        ) -> Option<transfer::Edge> {
            self.maps.get::<(FamilyId, TransferGraph)>(family)
                .and_then(|graph| graph.get(component))
        }

        fn update_transfer_graph(
            &self,
            family: &FamilyId,
            component: &ComponentType,
            edge: transfer::Edge,
        ) -> Result<(), DbError> {
            let mut guard = self.maps
                .mut_map::<(FamilyId, TransferGraph)>()
                .ok_or(DbError::FailedToAcquireMapping)?;
            
            guard.get_mut(family).and_then(|graph| graph.set(component, edge));

            Ok(())
        }

        /// Transfers an entity out of its current family into the provided family, copying all component data
        /// 
        /// This typically happens when a component is added or removed from the entity
        fn resolve_entity_transfer(
            &self,
            entity: &EntityId,
            family: &FamilyId
        ) -> Result<(), DbError> {

            let curr_family: FamilyId = self.maps.get::<(EntityId, FamilyId)>(entity)
                .ok_or(DbError::EntityDoesntExist(*entity))?;
            
            let dest_family: FamilyId = *family;

            // no change needs to be resolved
            if curr_family == dest_family {
                return Ok(())
            }
            
            // table guard scope
            {
                // try-back-off synchronization
                // guard scope
                loop {
                    let mut from_entity_map_guard;
                    let mut dest_entity_map_guard;
                    let from_columns_map_guard;
                    let dest_columns_map_guard;
                    let data_table_dest;
                    let data_table_from;

                    data_table_from = self.tables
                        .get(&curr_family)
                        .ok_or(DbError::TableDoesntExistForFamily(curr_family))?;

                    data_table_dest = self.tables
                        .get(&dest_family)
                        .ok_or(DbError::TableDoesntExistForFamily(dest_family))?;

                    if let Ok(col_from_guard) = data_table_from.columns().try_read() {
                        from_columns_map_guard = col_from_guard;
                    } else {
                        continue;
                    }
                    
                    if let Ok(col_dest_guard) = data_table_dest.columns().try_read() {
                        dest_columns_map_guard = col_dest_guard;
                    } else {
                        continue;
                    }

                    if let Ok(map_from_guard) = data_table_from.entity_map().try_write() {
                        from_entity_map_guard = map_from_guard;
                    } else {
                        continue;
                    }

                    if let Ok(map_dest_guard) = data_table_dest.entity_map().try_write() {
                        dest_entity_map_guard = map_dest_guard;
                    } else {
                        continue;
                    }

                    // If we've successfully locked everything we need this loop, we can proceeed
                    from_entity_map_guard.remove(entity);
                    
                    // The entity should exist in this table, as it has an index,
                    // we've removed its mapping in its old table, now lets move
                    // its data and then patch up the new tables mapping, as well as
                    // any other dbmaps we might be concerned about
                            
                    let new_entity_index = data_table_dest.get_next_free_row();

                    // Move the entity from one table to the other, column by column.
                    // This data is type-erased so we use a special "move" function pointer
                    // we created with each column that knows what to do
                    for (component_type, table_entry_from) in from_columns_map_guard.iter() {
                        if let Some(table_entry_dest) = dest_columns_map_guard.get(component_type) {
                            let index = (table_entry_from.fn_move)(entity, new_entity_index, &table_entry_from.data, &table_entry_dest.data)?;
                        } else {
                            data_table_dest.create_instance(entity, Some(new_entity_index), Some(&dest_columns_map_guard))?;
                            //deferred_table_construction  = true;
                            dest_entity_map_guard.insert(*entity, new_entity_index);
                            let table_entry_dest = dest_columns_map_guard.get(component_type).expect("expect just created column");
                            let index = (table_entry_from.fn_move)(entity, new_entity_index, &table_entry_from.data, &table_entry_dest.data)?;
                        }
                    }
                    
                    // patch db maps
                    // guard scope
                    {
                        let mut guard = self.maps
                            .mut_map::<(EntityId, FamilyId)>()
                            .ok_or(DbError::FailedToAcquireMapping)?;
                        guard.insert(*entity, dest_family);
                    }
                    drop(from_columns_map_guard);
                    drop(dest_columns_map_guard);

                    return Ok(())
                }
            }
        }

        /// Explicitly sets a component for an entity. This is often the first time
        /// a real component of a given type is created to an entity/family, and thus
        /// we may need to actually initialize the data column in the table we are
        /// trying to set data for
        fn set_component_for<C: Component>(
            &self,
            entity: &EntityId,
            component: C
        ) -> Result<(), DbError> {
            let family = self.maps.get::<(EntityId, FamilyId)>(entity)
                .ok_or(DbError::EntityBelongsToUnknownFamily)?;

            {
                self.tables
                    .get_mut(&family)
                    .ok_or(DbError::TableDoesntExistForFamily(family))?
                    .set_component(entity, component)?;
            }
            Ok(())
        }
    }

    impl Display for EntityDatabase {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "db dump\n")?;
            {
                for item in self.tables.iter() {
                    write!(f, "{}", *item);
                }
            }
            write!(f, "\n")
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
        use std::collections::HashMap;
        use std::sync::Arc;
        use std::sync::RwLock;
        use crate::database::reckoning::CType;
        use super::FamilyId;
        use super::ComponentType;
        
        #[derive(Clone)]
        pub struct TransferGraph {
            links: Arc<RwLock<HashMap<CType, Edge>>>,
        }

        impl TransferGraph {
            pub fn new() -> Self {
                Self {
                    links: Arc::new(RwLock::new(HashMap::new()))
                }
            }

            pub fn get(&self, component: &ComponentType) -> Option<Edge> {
                match self.links.read() {
                    Ok(graph) => {
                        graph.get(component).cloned()
                    },
                    Err(e) => {
                        panic!("unable to read transfer graph - rwlock - {:?}", e)
                    }
                }
            }

            pub fn set(&self, component: &ComponentType, edge: Edge) -> Option<Edge> {
                match self.links.write() {
                    Ok(mut graph) => {
                        graph.insert(*component, edge)
                    },
                    Err(e) => {
                        panic!("unable to set transfer graph - rwlock - {:?}", e)
                    }
                }
            }
        }

        #[derive(Clone)]
        pub enum Edge {
            Add(FamilyId),
            Remove(FamilyId),
        }
    } // transfer ======================================================================
    
    /// Macros
    /// 
    /// These macros make it possible to perform fast and ergonomic
    /// selections of data in an [super::EntityDatabase] 
    #[macro_use]
    pub mod macros {
        #[macro_export]
        macro_rules! impl_transformations {
            ($([$t:ident, $i:tt]),+) => {
                impl<'db, $($t,)+> Iterator for RowIter<'db, ($($t,)+)>
                where
                    $(
                        $t: MetaData,
                        $t: SelectOne<'db>,
                        <$t as SelectOne<'db>>::Type: Component,
                    )+
                {
                    type Item = ($($t::Ref,)+);
                    
                    fn next(&mut self) -> Option<Self::Item> {
                        todo!()
                    }
                }

                impl<'db, $($t,)+> IntoIterator for crate::database::Rows<'db, ($($t,)+)>
                where
                    $(
                        $t: MetaData,
                        $t: SelectOne<'db>,
                        <$t as SelectOne<'db>>::Type: Component,
                        $t: 'static,
                    )+
                {
                    type Item = ($($t::Ref,)+);
                    type IntoIter = RowIter<'db, ($($t,)+)>;

                    fn into_iter(self) -> Self::IntoIter {
                        
                        #![allow(unreachable_code, unused_variables)] todo!("into_iter");


                        let db = self.database();
                        let fs = self.families();


                        /*
                         * TODO:
                         * 
                         * INSTEAD of creating an iterator directly, we can create a 
                         * collection of jobs to be run, each job correlates to one
                         * transformation to be run on one family. The job pool can
                         * then chew through all of the jobs in parallel, demarcated
                         * by Phases. Threads with downtime between Phases can
                         * pull background jobs, or pull jobs which populate the next
                         * set of transformation jobs
                         * 
                         */


                        
                        //RowIter::new(db, fs)
                        todo!()
                    }
                }
                
                impl<'a, $($t,)+> const crate::database::Selection for ($($t,)+)
                where
                    $(
                        $t: MetaData,
                        $t: ~const SelectOne<'a>,
                    )+
                {
                    const READS: &'static [Option<ComponentType>] = &[$($t::reads(),)+];
                    const WRITES: &'static [Option<ComponentType>] = &[$($t::writes(),)+];
                }
            };
        }
    } // macros ========================================================================
} // reckoning =========================================================================

// Exports
pub use crate::transform;
pub use crate::transform::Rows;
pub use crate::transform::MetaData;
pub use crate::transform::SelectOne;
pub use crate::transform::Selection;
pub use crate::transform::RowIter;
pub use crate::conflict;
pub use crate::conflict::ConflictGraph;
pub use crate::conflict::ConflictColor;
pub use crate::conflict::Dependent;
pub use crate::database::reckoning::Component;
pub use crate::database::reckoning::ComponentType;
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

// tests ===============================================================================

#[cfg(test)]
#[allow(dead_code)]
mod vehicle_example {
    #[allow(dead_code)]
    #[allow(unused_assignments)]
    #[allow(unused_variables)]

    use super::reckoning::*;
    use super::transform::*;

    #[derive(Debug, Default)]
    pub struct Physics {
        pos: f64,
        vel: f64,
        acc: f64,
    }

    impl Component for Physics {}
    impl Physics {
        fn new() -> Self {
            Default::default()
        }
    }
    
    #[derive(Debug, Default)]
    pub struct Wheels {
        friction: f64,
        torque: f64,
        rpm: f64,
        radius: f64,
    }
    impl Component for Wheels {}
    
    #[derive(Debug, Default)]
    pub struct Chassis {
        weight: f64,
    }
    impl Component for Chassis {}

    #[derive(Clone, Debug, Default)]
    pub struct Engine {
        power: f64,
        torque: f64,
        rpm: f64,
        maxrpm: f64,
        throttle: f64,
    }
    impl Component for Engine {}

    #[derive(Debug, Default)]
    pub struct Transmission {
        gears: Vec<f64>,
        current_gear: Option<usize>,
    }
    impl Component for Transmission {}

    #[derive(Debug, Default)]
    pub enum Driver {
        #[default]
        SlowAndSteady,
        PedalToTheMetal,
    }
    impl Component for Driver {}
    
    struct DriveTrain;
    impl Transformation for DriveTrain {
        type Data = (Read<Engine>, Read<Transmission>, Write<Wheels>);

        fn run(data: Rows<Self::Data>) -> TransformationResult {
            println!("running drive-train transformation");

            // calculate engine torque & rpm

            for (engine, transmission, wheels) in data {
                if let Some(gear) = transmission.current_gear {
                    if let Some(gear_ratio) = transmission.gears.get(gear) {
                        wheels.torque = engine.torque * gear_ratio
                    }
                }
            }
            Ok(())
        }
    }
    
    struct DriverInput;
    impl Transformation for DriverInput {
        type Data = (Read<Driver>, Write<Transmission>, Write<Engine>);

        fn run(data: Rows<Self::Data>) -> TransformationResult {
            println!("running driver transformation");
            
            for (driver, transmission, engine) in data {
                match driver {
                    Driver::SlowAndSteady => {
                        match engine.rpm {
                            0.0..=5000.0 => {
                                match transmission.current_gear {
                                    Some(gear) => {
                                        engine.throttle = 0.4
                                    },
                                    None => {
                                        transmission.current_gear = Some(0)
                                    }
                                }
                            },
                            5000.0.. => {
                                engine.throttle = 0.0;
                                if let Some(gear) = transmission.current_gear {
                                    if gear < transmission.gears.len() {
                                        transmission.current_gear = Some(gear + 1)
                                    }
                                }
                            },
                            _ => {
                                engine.throttle = 0.0
                            }
                        }
                    },
                    Driver::PedalToTheMetal => {
                        match engine.rpm {
                            0.0..=5000.0 => {
                                match transmission.current_gear {
                                    Some(gear) => {
                                        engine.throttle = 1.0
                                    },
                                    None => {
                                        transmission.current_gear = Some(0)
                                    }
                                }
                            },
                            5000.0.. => {
                                engine.throttle = 0.0;
                                if let Some(gear) = transmission.current_gear {
                                    if gear < transmission.gears.len() {
                                        transmission.current_gear = Some(gear + 1)
                                    }
                                }
                            },
                            _ => {
                                engine.throttle = 0.0
                            }
                        }
                    },
                }
            }

            Ok(())
        }        
    }
    
    struct WheelPhysics;
    impl Transformation for WheelPhysics {
        type Data = (Write<Wheels>, Read<Chassis>, Write<Physics>);

        fn run(data: Rows<Self::Data>) -> TransformationResult {
            println!("running wheel physics transformation");

            for (wheels, chassis, physics) in data {
                physics.acc = wheels.torque / wheels.radius / chassis.weight;
                physics.vel += physics.acc;
                physics.pos += physics.vel;
                wheels.rpm = physics.vel * (60.0 / (2.0 * 3.14159) * wheels.radius);
            }
            Ok(())
        }
    }

    #[test]
    fn vehicle_example() {
        std::env::set_var("RUST_BACKTRACE", "1");
        
        let mut db = EntityDatabase::new();
        
        println!("\n\n\n\n{}\n\n\n\n", db);

        // Define some components from data, these could be loaded from a file
        let v8_engine = Engine { power: 400.0, torque: 190.0, rpm: 0.0, maxrpm: 5600.0, throttle: 0.0 };
        let diesel_engine = Engine { power: 300.0, torque: 650.0, rpm: 0.0, maxrpm: 3200.0, throttle: 0.0 };

        let heavy_chassis = Chassis { weight: 7000.0, };
        let sport_chassis = Chassis { weight: 2200.0, };

        let five_speed = Transmission {
            gears: vec![2.95, 1.94, 1.34, 1.00, 0.73],
            current_gear: None,
        };

        let ten_speed = Transmission {
            gears: vec![4.69, 2.98, 2.14, 1.76, 1.52, 1.27, 1.00, 0.85, 0.68, 0.63],
            current_gear: None,
        };
        
        // Build the entities from the components we choose
        // This can be automated from data
        let sports_car = db.create().unwrap();
        println!("\n\n\n\n{}\n\n\n\n", db);
        db.add_component(sports_car, v8_engine.clone()).unwrap();
        println!("\n\n\n\n{}\n\n\n\n", db);

        db.add_component(sports_car, five_speed).unwrap();
        db.add_component(sports_car, sport_chassis).unwrap();
        db.add_component(sports_car, Wheels::default()).unwrap();
        db.add_component(sports_car, Physics::new()).unwrap();
        db.add_component(sports_car, Driver::PedalToTheMetal).unwrap();

        let pickup_truck = db.create().unwrap();
        db.add_component(pickup_truck, v8_engine).unwrap();
        db.add_component(pickup_truck, ten_speed).unwrap();
        db.add_component(pickup_truck, heavy_chassis).unwrap();
        db.add_component(pickup_truck, Wheels::default()).unwrap();
        db.add_component(pickup_truck, Physics::new()).unwrap();
        db.add_component(pickup_truck, Driver::SlowAndSteady).unwrap();

        // Let's swap the engine in the truck for something more heavy duty
        db.add_component(pickup_truck, diesel_engine).unwrap();

        // Create a simulation phase. It is important to note that things
        // that happen in a single phase are unordered. If it is important
        // for a certain set of transformations to happen before or after
        // another set of transformations, you must break them into distinct
        // phases. Each phase will run sequentially, and each transformation
        // within a phase will (try to) run in parallel 
        let mut sim_phase = Phase::new();
        sim_phase.add_transformation(DriveTrain);
        sim_phase.add_transformation(WheelPhysics);
        sim_phase.add_transformation(DriverInput);
        
        // The simulation loop. Here we can see that, fundamentally, the
        // simulation is nothing but a set of transformations on our
        // dataset run over and over. By adding more components and
        // transformations to the simulation we expand its capabilities
        // while automatically leveraging parallelism
        loop {
            sim_phase.run_on(&db).unwrap();
            
            // Here we allow the database to communicate back with the
            // simulation loop through commands
            while let Some(command) = db.query_commands() {
                match command {
                    Command::Quit => break,
                }
            }

            break;
        }
    }
}

// CLEAR: "\x1B[2J\x1B[1;H"
