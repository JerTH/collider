use std::{
    collections::HashMap,
    fmt::{self, Display},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex, RwLock,
    },
};

use dashmap::DashMap;

use crate::{
    borrowed::{ColumnIter, ColumnIterMut},
    database::{
        reckoning::{AnyPtr, ColumnReadGuard, ColumnWriteGuard, DbError, ComponentTypeSet},
        Component, ComponentType,
    },
    id::{FamilyId, StableTypeId},
    EntityId, column::{Column, ColumnKey},
};

/// Type erased entry into a table which describes a single column
/// A column contains one type of component. An single index into a table describes
/// an entity made up of different components in different columns
pub struct TableEntry {
    pub tyid: StableTypeId, // The type id of Column<T>  ([Self::data])
    pub data: AnyPtr,       // Column<T>

    pub fn_constructor: fn() -> AnyPtr,
    pub fn_instance: fn(&AnyPtr, usize),
    pub fn_move: fn(&EntityId, usize, &AnyPtr, &AnyPtr) -> Result<usize, DbError>,
    pub fn_resize: fn(&AnyPtr, usize) -> usize,
}

impl<'b> TableEntry {
    pub fn resize_minimum(&self, min_size: usize) -> usize {
        (self.fn_resize)(&self.data, min_size)
    }
    
}

impl Display for TableEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "table entry")?;
        write!(
            f,
            " type: {} - {}",
            self.tyid.0,
            self.tyid.name().unwrap_or("{unknown name}")
        )
    }
}

pub struct Table {
    family: FamilyId,
    components: ComponentTypeSet,

    /// The set of keys used to access the columns owned by this table
    columns: HashMap<ComponentType, ColumnKey>,

    /// Maps entity ID's to their real index value
    /// This can/should be improved in the future - entity id's have
    /// ability to store indexing information intrinsically
    entity_map: HashMap<EntityId, usize>,

    /// Free list of indicies, the free list is populated whenever
    /// an entity is destroyed or moved out of the table
    free: Vec<usize>,
}

impl Table {
    pub fn new(family: FamilyId, components: ComponentTypeSet) -> Self {
        Table {
            family,
            components,
            columns: HashMap::new(),
            entity_map: HashMap::new(),
            free: Vec::new(),
        }
    }

    pub fn free_count(&self) -> usize {
        return self.free.len()
    }

    pub fn entity_map(&self) -> &HashMap<EntityId, usize> {
        return &self.entity_map;
    }
    
    pub fn column_map(&self) -> &HashMap<ComponentType, ColumnKey> {
        &self.columns
    }

    /// Gets the index of the next free row in the table
    /// expanding the table if necessary
    pub fn get_next_free_row(&self) -> usize {
        // Do we already have a next free row?
        if let Some(free) = self.free.pop() {
            return free
        } else {
            return self.entity_map.len()
        }
    }

    pub fn insert_new_entity(&mut self, entity: &EntityId) -> Result<usize, DbError> {
        let index = self.get_next_free_row();
        self.entity_map().insert(*entity, index);
        Ok(index)
    }
}

impl Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\nTable\n")?;
        write!(f, "family: {}\n", self.family)?;
        write!(f, "size: {}\n", self.entity_map.len());
        write!(f, "num_free: {}\n", self.free.len());

        {
            write!(f, "entity_map:\n")?;
            for item in self.entity_map() {
                write!(f, " ({} : {})\n", item.0, item.1)?;
            }
        }
        write!(f, "\n")
    }
}