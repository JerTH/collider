use std::{fmt::{self, Display}, collections::HashMap, sync::{RwLock, atomic::{AtomicUsize, Ordering}, Mutex}};

use crate::{database::{reckoning::{AnyPtr, DbError, ColumnReadGuard, ColumnWriteGuard}, Component, ComponentType}, id::{StableTypeId, FamilyId}, EntityId, borrowed::{ColumnIter, Column, ColumnIterMut}};



/// Type erased entry into a table which describes a single column
/// A column contains one type of component. An single index into a table describes 
/// an entity made up of different components in different columns
pub struct TableEntry {
    pub tyid: StableTypeId, // The type id of Column<T>  ([Self::data])
    pub data: AnyPtr, // Column<T>
    
    pub fn_constructor: fn() -> AnyPtr,
    pub fn_instance: fn(&AnyPtr, usize),
    pub fn_move: fn(&EntityId, usize, &AnyPtr, &AnyPtr) -> Result<usize, DbError>,
    pub fn_resize: fn(&AnyPtr, usize) -> usize,
}

impl<'b> TableEntry {
    pub fn resize_minimum(&self, min_size: usize) -> usize {
        (self.fn_resize)(&self.data, min_size)
    }
    pub fn iter<T: Component>(&'b self) -> ColumnIter<'b, T> {
        debug_assert!(StableTypeId::of::<T>() == self.tyid);
        let column = self.data.downcast_ref::<Column<T>>()
            .expect("expected matching column types");
        let column_ref = column.borrow_column();
        let column_iter = column_ref.into_iter();
        column_iter
    }
    
    pub fn iter_mut<T: Component>(&'b self) -> ColumnIterMut<'b, T> {
        debug_assert!(StableTypeId::of::<T>() == self.tyid);
        let column = self.data.downcast_ref::<Column<T>>()
            .expect("expected matching column types");
        let column_mut = column.borrow_column_mut();
        let column_iter_mut = column_mut.into_iter();
        
        column_iter_mut
    }
}

impl Display for TableEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "table entry")?;
        write!(f, " type: {} - {}", self.tyid.0, self.tyid.name().unwrap_or("{unknown name}"))
    }
}

pub struct Table {
    family: FamilyId,
    columns: RwLock<HashMap<ComponentType, TableEntry>>, // hashmap never changes after the table is fully initialized
    entitym: RwLock<HashMap<EntityId, usize>>,
    numfree: AtomicUsize,
    free: Mutex<Vec<usize>>,
    size: AtomicUsize, // this should be equal to entitym.len()
}

impl Table {
    pub fn new(family: FamilyId) -> Self {
        Table {
            family: family,
            columns: RwLock::new(HashMap::new()),
            entitym: RwLock::new(HashMap::new()),
            free: Mutex::new(Vec::new()),
            numfree: AtomicUsize::new(0),
            size: AtomicUsize::new(0),
        }
    }

    /// Returns a new [Table] with empty columns, retaining only the information needed to construct
    /// instances of the type erased columns comprising the original [Table]
    pub fn clone_new(&self, family: FamilyId) -> Self {
        let columns = self.columns
            .read()
            .expect("unable to lock table columns for read")
            .iter()
            .map(|(tyid, entry)| {
                (*tyid, TableEntry {
                    tyid: entry.tyid,
                    data: Box::new((entry.fn_constructor)()),
                    fn_constructor: entry.fn_constructor,
                    fn_instance: entry.fn_instance,
                    fn_move: entry.fn_move,
                    fn_resize: entry.fn_resize,
                })
            })
            .collect();
        
        Table {
            family,
            columns: RwLock::new(columns),
            entitym: RwLock::new(HashMap::new()),
            numfree: AtomicUsize::new(0),
            free: Mutex::new(Vec::new()),
            size: AtomicUsize::new(0),
        }
    }

    pub fn columns(&self) -> &RwLock<HashMap<ComponentType, TableEntry>> {
        return &self.columns
    }

    pub fn entity_map(&self) -> &RwLock<HashMap<EntityId, usize>> {
        return &self.entitym
    }

    /// Gets the index of the next free row in the table
    /// expanding the table if necessary
    pub fn get_next_free_row(&self) -> usize {
        // Do we already have a next free row?
        if self.numfree.load(Ordering::SeqCst) > 0 {
            match self.free.lock() {
                Ok(mut guard) => {
                    if let Some(free_row) = guard.pop() {
                        self.numfree.fetch_sub(1, Ordering::SeqCst);
                        return free_row
                    }
                },
                Err(e) => {
                    panic!("unable tot lock table free row mutex: {:?}", e);
                }
            }
        }
        return self.size.load(Ordering::SeqCst);
    }

    /// Sets a component for an entity
    pub fn set_component<C: Component>(
        &self,
        entity: &EntityId,
        component: C
    ) -> Result<(), DbError>
    {
        let component_type = ComponentType::of::<C>();
        
        let index = *self.entitym
            .read()
            .expect("unable to read table entity map")
            .get(entity)
            .ok_or(DbError::EntityNotInTable(*entity, self.family))?;
        let mut column_guard = self.columns.write().expect("unable to acquire column lock");
        
        let table_entry = column_guard.entry(component_type).or_insert_with(|| {
            let column: Column<C> = Column::new();
            
            let table_entry = TableEntry {
                tyid: StableTypeId::of::<C>(),
                data: Box::new(column),
                fn_constructor: Column::<C>::dynamic_ctor,
                fn_instance: Column::<C>::dynamic_instance,
                fn_move: Column::<C>::dynamic_move,
                fn_resize: Column::<C>::dynamic_resize,
            };
            table_entry.resize_minimum(index);
            table_entry
        });
        let column = table_entry.data.downcast_ref::<Column<C>>()
            .ok_or(DbError::ColumnTypeDiscrepancy)?;
        
        // Borrow the column, panics if it is already borrowed
        let mut column_ref = column.borrow_column_mut();
        column_ref[index] = component;
        Ok(())
    }

    fn move_row(row: &EntityId, dest: &Table) {
        //for component in self.
        panic!()
    }

    pub fn create_instance(&self, entity: &EntityId, index: Option<usize>, column_guard: Option<&ColumnReadGuard>) -> Result<usize, DbError> {
        let index = index.unwrap_or(self.get_next_free_row());
        
        match column_guard {
            Some(column_guard) => {
                column_guard.iter().for_each(|(_, table_entry)| {
                    let column = &table_entry.data;
                    (table_entry.fn_instance)(column, index)
                });
            },
            None => {
                let column_guard = self.columns
                    .read()
                    .expect("unable to acquire column read lock");
                column_guard.iter().for_each(|(_, table_entry)| {
                    let column = &table_entry.data;
                    (table_entry.fn_instance)(column, index)
                });
            }
        }
        Ok(index)
    }

    fn resize_minimum(&self, min_size: usize, column_guard: &ColumnWriteGuard) -> usize {
        let mut new_size = 0;
        for (_ty, table_entry) in column_guard.iter() {
            new_size = table_entry.resize_minimum(min_size);
        }
        return new_size;
    }
}

impl Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\nTABLE\n")?;
        write!(f, "family: {}\n", self.family)?;
        write!(f, "size: {}\n", self.size.load(Ordering::SeqCst))?;
        write!(f, "num_free: {}\n", self.numfree.load(Ordering::SeqCst))?;
        
        {
            write!(f, "entity_map:\n")?;
            if let Ok(entity_map) = self.entitym.read() {
                for item in entity_map.iter() {
                    write!(f, " {}->{}\n", item.0, item.1)?;
                }
            }
        }
        write!(f, "columns:\n")?;
        let column_guard = self.columns.read().expect("unable to acquire column read lock");
        for item in column_guard.iter() {
            write!(f, " {}\n {}", item.0, item.1)?;
        }
        write!(f, "\n")
    }
}
