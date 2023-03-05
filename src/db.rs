use std::any::Any;
use std::ops::DerefMut;
use std::ops::Deref;
use std::sync::Mutex;
use std::sync::Arc;
use std::sync::MutexGuard;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::Debug;
use crate::family::SubFamilies;
use crate::family::SubFamilyMap;
use crate::id::StableTypeId;
use crate::id::FamilyId;
use crate::id::EntityId;
use crate::family::FamilyDelta;
use crate::family::FamilyGraphEdge;
use crate::family::FamilyTransform;
use crate::family::EntityFamilyMap;
use crate::family::ContainingFamiliesMap;
use crate::family::ComponentSetFamilyMap;
use crate::family::Family;
use crate::comps::ComponentDelta;
use crate::comps::ComponentTypeSet;
use crate::comps::ComponentType;
use crate::comps::ComponentRegistry;
use crate::comps::Component;
use crate::xform::Read;
use crate::xform::Selection;
use crate::xform::Write;

/// The database itself. This is the interface for consuming code
#[derive(Debug)]
pub struct EntityDatabase {
    data: EntityData,
    records: EntityRecords,
    alloc: EntityAllocator,
    registry: ComponentRegistry,
}

/// Dumb storage which owns the raw data that entities are made of
#[derive(Debug)]
struct EntityData {
    tables: HashMap<FamilyId, DataTable>,
    families: HashMap<FamilyId, Family>,
}

/// Stores and tracks of all bookkeeping data used to manage and query entities
#[derive(Debug)]
struct EntityRecords {
    component_sets: ComponentSetFamilyMap,
    containing: ContainingFamiliesMap,
    entities: EntityFamilyMap,
    sub_families: SubFamilyMap,
}

/// Provides `EntityId`'s and manages their re-use
#[derive(Debug)]
struct EntityAllocator {
    count: u32,
    free: Vec<EntityId>,
}

/// A `ComponentColumn<T>` is the raw typed storage for a single component type in a `DataTable`
/// 
/// Columns are indexed by `ComponentType`'s, and rows are indexed by `EntityId`'s
#[derive(Default)]
pub(crate) struct ComponentColumn<T: Component>(pub HashMap<EntityId, T>);


/// Stores raw component data of a single type, describes how to interact with that data
pub struct ComponentColumnEntry {
    pub data: Box<dyn Any>, // ComponentColumn<T>
    mvfn: fn(&EntityId, &mut Box<dyn Any>, &mut Box<dyn Any>),
    ctor: fn() -> Box<dyn Any>,
}

type TableEntry = HashMap<ComponentType, ComponentColumnEntry>;

#[derive(Debug)]
pub(crate) struct DataTableGuard<'a>(MutexGuard<'a, TableEntry>);

// Example flow of accessing a component:
//
//fn foo<'a>() -> Option<&'a ()> {
//    type UnitComponent = ();
//    let table = DataTable::default();
//    let guard = table.lock();
//    let entity = EntityId::generational(0, 0, 0, 0);
//    let component = ComponentType::of::<UnitComponent>();
//    match guard.get(&component) {
//        Some(entry) => {
//            match entry.data.downcast_ref::<ComponentColumn<UnitComponent>>() {
//                Some(column) => {
//                    column.get(&entity)
//                },
//                None => todo!(),
//            }
//        },
//        None => todo!(),
//    }
//}

/// A `DataTable` represents a single table of components unique to a family. This is where actual user
/// application data is stored as rows and columns of components, and so its contents are dynamically typed
/// 
/// Each column stores one kind of component, and each row represents one logical entity
/// 
/// `DataTable`'s exist on a one-to-one basis with `Family`'s. Each `FamilyId` points to a single `DataTable` 
/// 
/// The `DataTable` is indexed column-first, where each column represents a component type and rows are entities
///     Name   Health
/// E30 [&'1]  [ 5 ] 
/// E12 [&'2]  [ 8 ] 
/// E77 [&'2]  [ 2 ] 
/// E31 [&'1]  [ 0 ]
///  
#[derive(Default, Debug)]
struct DataTable(Arc<Mutex<TableEntry>>);

// Impl's

// `EntityDatabase`
impl EntityDatabase {
    /// Creates a new empty `EntityDatabase`.
    /// 
    /// Automatically registers a special unit component of type `()`
    pub fn new() -> Self {
        let mut db = EntityDatabase {
            data: EntityData::new(),
            records: EntityRecords::new(),
            alloc: EntityAllocator::new(),
            registry: ComponentRegistry::new(),
        };

        // Register the unit/null component
        db.register_component_debug_info::<()>();
        db
    }
    
    /// Creates a new entity with no components, returning its unique `EntityId`
    pub fn create(&mut self) -> EntityId {
        self.alloc.alloc()
    }
    
    fn component_set_of(&self, family: &FamilyId) -> Option<ComponentTypeSet> {
        self.data.families.get(family).and_then(|f| Some(f.components()))
    }
    
    fn family_of_component_set(&self, set: &ComponentTypeSet) -> Option<FamilyId> {
        self.records.component_sets.get(set).cloned()
    }

    pub(crate) fn try_lock_family_table(&self, family_id: &FamilyId) -> Option<DataTableGuard> {
        self.data.tables.get(&family_id)?
            .try_lock().ok()
            .map(|guard| DataTableGuard(guard))
    }

    /// Attempts to find the `FamilyId` for the family the entity would belong to after the component delta is applied
    /// 
    /// Returns `None` if no suitable family exists
    fn get_family_transform(&self, entity: &EntityId, delta: ComponentDelta) -> FamilyTransform {
        
        // TODO: Clean up/flatten/rustify this mess of nested match statements
        match delta {
            ComponentDelta::Add(added) => {
                // first things first, does the entity exist as far as the database is concerned?
                match self.records.entities.get(entity) {
                    Some(current_family_id) => {
                        // the entity exists, check its transfer graph for a link
                        match self.data.families.get(&current_family_id) {
                            Some(family) => {
                                match family.try_get_transfer_edge(&added) {
                                    // if we have a valid graph edge for adding this component, use it
                                    Some(FamilyGraphEdge{delta: FamilyDelta::Add(graph_target), ..}) => {
                                        return FamilyTransform::Transfer { from: *current_family_id, dest: graph_target }
                                    },
                                    // otherwise, try to find a suitable family using the component sets
                                    // todo: interior mutability to update the transfer graph here
                                    _ => {
                                        match self.component_set_of(current_family_id) {
                                            Some(set) => {
                                                if set.contains(&added) {
                                                    return FamilyTransform::NoChange(*current_family_id)
                                                } else {
                                                    // we're transfering this entity into a new family, which one?
                                                    let set = set.iter().cloned().chain([added].iter().cloned()).collect();
                                                    match self.family_of_component_set(&set) {
                                                        Some(new_family_id) => {
                                                            return FamilyTransform::Transfer { from: *current_family_id, dest: new_family_id }
                                                        },
                                                        None => {
                                                            return FamilyTransform::InitTransfer { from: *current_family_id, set: set }
                                                        },
                                                    }
                                                }
                                            },
                                            None => {
                                                panic!("family data doesn't exist or is invalid")
                                            },
                                        }
                                    }
                                }
                            },
                            None => {
                                panic!("family data doesn't exist or is invalid")
                            },
                        }
                    },
                    None => {
                        // the entity doesn't currently exist, if we create it, where does it belong?
                        let set = ComponentTypeSet::from(added);
                        match self.family_of_component_set(&set) {
                            Some(family_id) => {
                                return FamilyTransform::NewEntity(family_id)
                            },
                            None => {
                                return FamilyTransform::InitNew(set)
                            },
                        }
                    },
                }
            },
            ComponentDelta::Remove(removed) => {
                todo!()
            },
        }
    }

    /// Inserts or replaces component data for a given entity in the appropriate `DataTable`, associated
    /// by the entities `Family`. Lazily constructs data tables component columns
    fn insert_real_component<T: Component>(&mut self, entity: &EntityId, family: &FamilyId, component: T) -> Result<(), ()> {
        let table = self.data.tables.get_mut(&family).ok_or(())?;
        let mut guard = table.lock();
        
        match guard.entry(component!(T)) {
            Entry::Occupied(mut occupied) => {
                let column_entry = occupied.get_mut();
                match column_entry.data.downcast_mut::<ComponentColumn<T>>() {
                    Some(column) => {
                        column.insert(*entity, component);
                    },
                    None => {
                        panic!("mismatched column type");
                    },
                }
            },
            Entry::Vacant(vacant) => {
                let column = ComponentColumn::<T>::from((*entity, component));
                let column_entry = ComponentColumnEntry {
                    data: Box::new(column),
                    mvfn: ComponentColumn::<T>::fn_virtual_move,
                    ctor: ComponentColumn::<T>::fn_virtual_ctor,
                };

                vacant.insert(column_entry);
            },
        }
        Ok(())
    }

    fn set_family_record(&mut self, entity: &EntityId, family: &FamilyId) {
        self.records.entities.insert(*entity, *family);
    }

    /// Acquires locks on two `DataTable`'s at once. Attempts to avoid deadlocks by first acquiring `a`
    /// and then trying to acquire `b`, giving up the lock on `a` and starting over if `b` can not be
    /// immediately acquired. Locking two tables in an unknown order is necessary during data copies
    fn lock_tables_yielding<'a, 'b>(&self, a: &'a DataTable, b: &'b DataTable) -> (DataTableGuard<'a>, DataTableGuard<'b>) {
        loop {
            let ga = a.lock();
            match b.try_lock() {
                Ok(gb) => {
                    return (ga, DataTableGuard(gb))
                },
                Err(err) => {
                    std::mem::drop(ga)
                },
            }
        }
    }

    fn resolve_transform<T: Component>(&mut self, entity: EntityId, component: T, transform: FamilyTransform) -> Result<(), ()> {
        let dest_family = match transform {
            // the entity is being transfered from one family to another
            FamilyTransform::Transfer { from, dest } => {
                let dest_family = dest;

                {
                    let from = self.data.tables.get(&from).unwrap();
                    let dest = self.data.tables.get(&dest).unwrap();

                    let (mut from, mut dest) = self.lock_tables_yielding(from, dest);
                    from.move_row(&entity, &mut dest);
                }
                
                self.set_family_record(&entity, &dest_family);
                dest_family
            },

            // the entity is new, it doesn't have a family yet
            FamilyTransform::NewEntity(family_id) => {
                self.set_family_record(&entity, &family_id);
                family_id
            },

            // the entity already owns one of these components, we're just replacing it
            FamilyTransform::NoChange(current_family) => {
                current_family
            },

            // the entity is being transfered from one family to another, but we need to create the new family first
            FamilyTransform::InitTransfer { from, set } => {
                let dest = self.register_family(set);
                return self.resolve_transform(entity, component, FamilyTransform::Transfer { from, dest })
            }

            // the entity doesn't have a family yet and the family it should belong to doesn't exist
            FamilyTransform::InitNew(set) => {
                let family_id = self.register_family(set);
                self.set_family_record(&entity, &family_id);
                family_id
            },
        };

        self.insert_real_component::<T>(&entity, &dest_family, component)
    }

    /// Adds a component `T` to an entity by its `EntityId`
    /// 
    /// 
    pub fn add_component<T: Component>(&mut self, entity: EntityId, component: T) -> Result<(), ()> {
        if !self.registry.seen::<T>() {
            self.registry.register::<T>()
        }

        let component_type = component!(T);
        let transform = self.get_family_transform(&entity, ComponentDelta::Add(component_type));
        self.resolve_transform(entity, component, transform)
    }
    
    /// Registers a component with the `EntityDatabase` for richer debug info 
    #[deprecated(note = "handled by component registry")]
    pub fn register_component_debug_info<T: Component>(&mut self) {
        StableTypeId::register_debug_info::<T>();
        self.register_family(component_type_set!(T));
    }
    
    /// Registers a set of components as a `Family`
    fn register_family(&mut self, set: ComponentTypeSet) -> FamilyId {
        let id: FamilyId = self.alloc.alloc().into();

        
        self.data.tables
        .entry(id)
        .or_insert(DataTable::default());
    
        for comp in set.iter().cloned() {
            self.records.containing
            .entry(comp)
            .and_modify(|f| { f.insert(id); } )
            .or_insert(family_id_set!(id));
        }

        self.data.families
            .entry(id)
            .or_insert(Family::from(set.clone()));

        self.records.component_sets.insert(set, id);
        id
    }

    /// Selects a set of components with read or write access from the database
    /// 
    /// The selection only includes components of entities which hold all of the
    /// selected components, e.g. the family or sub-families derived from the list
    /// of components selected
    pub fn select<T: Selection>(&self) -> T {
        T::make(&self)
    }
    
    pub fn select_read<T>(&self) -> Read<T> where T: Component {
        Read::new()
    }

    pub fn select_write<T>(&self) -> Write<T> where T: Component {
        Write::new()
    }

    pub(crate) fn sub_families(&self, set: ComponentTypeSet) -> Option<SubFamilies> {
        self.records.sub_families.get(&set).cloned()
    }
}

// `EntityData`
impl EntityData {
    fn new() -> Self {
        EntityData { tables: HashMap::default(), families: HashMap::default() }
    }
}

// `EntityRecords`
impl EntityRecords {
    fn new() -> Self {
        EntityRecords {
            component_sets: ComponentSetFamilyMap::default(),
            containing: ContainingFamiliesMap::default(),
            entities: EntityFamilyMap::default(),
            sub_families: SubFamilyMap::default(),
        }
    }
}

// `EntityAllocator`
impl EntityAllocator {
    fn new() -> Self {
        EntityAllocator { count: 0u32, free: Default::default() }
    }

    fn alloc(&mut self) -> EntityId {
        let (idx, gen, m1, m2) = if let Some(id) = self.free.pop() {
            unsafe { id.generational }
        } else {
            self.count = self.count.checked_add(1u32).expect("too many entities");
            (self.count, 0u16, 0u8, 0u8)
        };
        EntityId::generational(idx, gen, m1, m2)
    }

    fn free(&mut self, id: EntityId) {
        let (idx, gen, m1, m2) = unsafe { id.generational };
        self.free.push(EntityId::generational(idx, gen.wrapping_add(1u16), m1, m2));
    }
}

// `ComponentColumn`
impl<T: Component> ComponentColumn<T> {
    fn new() -> Self {
        ComponentColumn(Default::default())
    }

    fn fn_virtual_move(row: &EntityId, from: &mut Box<dyn Any>, to: &mut Box<dyn Any>) {
        let from = from.downcast_mut::<ComponentColumn<T>>().expect("expect from");
        let dest = to.downcast_mut::<ComponentColumn<T>>().expect("expect dest");
        match from.remove(row) {
            Some(item) => { dest.insert(*row, item); },
            None => { #[cfg(Debug)] println!("no row value to move"); },
        }
    }

    fn fn_virtual_ctor() -> Box<dyn Any> {
        Box::new(ComponentColumn::<T>::new())
    }
}

impl<T: Component> From<(EntityId, T)> for ComponentColumn<T> {
    fn from(value: (EntityId, T)) -> Self {
        let mut column = ComponentColumn::new();
        column.insert(value.0, value.1);
        column
    }
}

impl<T: Component> Debug for ComponentColumn<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentColumn").field("data", &self).finish()
    }
}

impl<T: Component> Deref for ComponentColumn<T> {
    type Target = HashMap<EntityId, T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Component> DerefMut for ComponentColumn<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// `ComponentColumnEntry`
impl ComponentColumnEntry {
    fn insert<T: Component>(&mut self, key: EntityId, val: T) -> Option<T> {
        let column = self.data.downcast_mut::<ComponentColumn<T>>().unwrap();
        column.insert(key, val)
    }

    fn move_row_val(&mut self, row: &EntityId, dest: &mut Self) {
        (self.mvfn)(row, &mut self.data, &mut dest.data)
    }

    fn shallow_clone(&self) -> Self {
        let data = (self.ctor)();
        ComponentColumnEntry {
            data: data,
            mvfn: self.mvfn,
            ctor: self.ctor
        }
    }
}

impl Debug for ComponentColumnEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentColumnEntry")
        .field("data", &self.data)
        .finish()
    }
}

// `DataTableGuard`
impl<'a> DataTableGuard<'a> {
    fn move_row(&mut self, row: &EntityId, to: &mut DataTableGuard) {
        for (component, from) in self.iter_mut() {
            match to.entry(*component) {
                Entry::Occupied(mut occupied) => {
                    let to = occupied.get_mut();
                    from.move_row_val(row, to);
                },
                Entry::Vacant(vacant) => {
                    let new_column = vacant.insert(ComponentColumnEntry {
                        data: (from.ctor)(),
                        mvfn: from.mvfn,
                        ctor: from.ctor,
                    });
                    from.move_row_val(row, new_column);
                },
            }
            if let Some(to) = to.get_mut(component) {
                from.move_row_val(row, to);
            }
        }
    }
}

impl<'a> Deref for DataTableGuard<'a> {
    type Target = TableEntry;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl<'a> DerefMut for DataTableGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}

// `DataTable`
impl DataTable {
    fn lock(&self) -> DataTableGuard {
        match self.0.lock() {
            Ok(guard) => DataTableGuard(guard),
            Err(err) => panic!("poisoned, unable to lock data table for reading: {}", err),
        }
    }
}

impl Deref for DataTable {
    type Target = Arc<Mutex<TableEntry>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DataTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

