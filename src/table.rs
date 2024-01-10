use std::{
    collections::HashMap,
    fmt::{self, Display},
    sync::RwLock,
};

use collider_core::{id::{FamilyId, ColumnKey}, component::ComponentType};
use dashmap::DashMap;

use crate::{
    components::ComponentTypeSet,
    error::DbError,
    EntityId,
};

#[derive(Debug)]
pub struct Table {
    family: FamilyId,
    components: ComponentTypeSet,

    /// The set of keys used to access the columns owned by this table
    columns: HashMap<ComponentType, ColumnKey>,

    /// Maps entity ID's to their real index value
    /// This can/should be improved in the future - entity id's have
    /// ability to store indexing information intrinsically
    entity_map: DashMap<EntityId, usize>,

    /// Free list of indicies, the free list is populated whenever
    /// an entity is destroyed or moved out of the table
    free: RwLock<Vec<usize>>,
}

impl Table {
    /// Creates a new [`Table`].
    pub(crate) fn new(family: FamilyId, components: ComponentTypeSet) -> Self {
        Table {
            family,
            components,
            columns: HashMap::new(),
            entity_map: DashMap::new(),
            free: RwLock::new(Vec::new()),
        }
    }
    
    /// Returns a reference to the family id of this [`Table`].
    pub fn family_id(&self) -> &FamilyId {
        &self.family
    }
    
    //pub fn free_count(&self) -> usize {
    //    self.free
    //        .read()
    //        .expect("unable to read table free list")
    //        .len()
    //}

    /// Returns a reference to the entity map of this [`Table`].
    pub fn entity_map(&self) -> &DashMap<EntityId, usize> {
        &self.entity_map
    }

    /// Returns a reference to the column map of this [`Table`].
    pub fn column_map(&self) -> &HashMap<ComponentType, ColumnKey> {
        &self.columns
    }

    /// Returns a reference to the components of this [`Table`].
    pub fn components(&self) -> &ComponentTypeSet {
        &self.components
    }
    
    pub fn remove_entity(&mut self, entity: &EntityId) -> Result<(), DbError> {
        //let index = *self
        //    .entity_map
        //    .get(entity)
        //    .ok_or(DbError::EntityNotInTable(*entity, self.family))?;
        //
        //self.free
        //    .write()
        //    .expect("unable to lock table free list for writes")
        //    .push(index);

        self.entity_map.remove(entity);

        //self.entity_map
        //    .remove(entity)
        //    .expect("expected entity map to contain this value because of previous access");

        Ok(())
    }
    
    /// Gets the index of the next free row in the table
    /// expanding the table if necessary
    pub fn get_next_free_row(&self) -> usize {
        // Do we already have a next free row?
        return self.entity_map().len();

        //if let Some(free) = self
        //    .free
        //    .write()
        //    .expect("unable to write table free list")
        //    .pop()
        //{
        //    //println!("HAS INDEX FROM FREE LIST: {}", free);
        //    return free;
        //} else {
        //    return self.entity_map.len();
        //}
    }
    
    pub fn insert_new_entity(&mut self, entity: &EntityId) -> Result<usize, DbError> {
        let index = self.get_next_free_row();
        self.entity_map.insert(*entity, index);
        Ok(index)
    }

    pub(crate) fn get_or_insert_entity(&mut self, entity: &EntityId) -> TableResult<DbError> {
        if let Some(index) = self.entity_map().get(entity) {
            return TableResult::EntityAlreadyExists(*index)
        }

        match self.insert_new_entity(entity) {
            Ok(index) => {
                return TableResult::EntityInserted(index)
            },
            Err(e) => {
                return TableResult::Error(e)
            },
        }
    }

    pub fn update_column_map(&mut self, component_type: ComponentType, column_key: ColumnKey) {
        self.columns.insert(component_type, column_key);
    }
}

#[derive(Debug)]
pub(crate) enum TableResult<E> {
    EntityAlreadyExists(usize),
    EntityInserted(usize),
    Error(E),
}

impl Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Table\n")?;
        write!(f, "family: {}\n", self.family)?;
        write!(f, "components: {:#?}\n", self.components)?;
        write!(f, "size: {}\n", self.entity_map.len())?;
        write!(
            f,
            "num_free: {}\n",
            self.free
                .read()
                .expect("unable to read table free list")
                .len()
        )?;
        write!(f, "entity_map:\n")?;
        for item in self.entity_map() {
            write!(f, " ({} : {})\n", item.key(), item.value())?;
        }
        write!(f, "\n")
    }
}
