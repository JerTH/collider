#[allow(dead_code)] // during re-write only

#[macro_use]
pub mod reckoning {
    use core::fmt;
    // misc
    use std::any::Any;
    use std::any::type_name;
    use std::borrow::BorrowMut;
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
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    
    // collections
    use std::collections::HashSet;
    use std::collections::BTreeSet;
    use std::collections::HashMap;

    // crate
    use crate::EntityId;
    use crate::id::{StableTypeId, IdUnion};

    // typedefs
    type CType = self::ComponentType;
    type CTypeSet = self::ComponentTypeSet;
    type ColumnReadGuard<'a> = RwLockReadGuard<'a, HashMap<CType, TableEntry>>;
    type ColumnWriteGuard<'a> = RwLockWriteGuard<'a, HashMap<CType, TableEntry>>;
    type AnyPtr = Box<dyn Any>;
    type CommutativeHashValue = u64;
    type FamilyIdSetImpl = HashSet<FamilyId>; // INVARIANT: This type MUST NOT accept duplicates 
    type FamilyIdSetInner = (CommutativeId, FamilyIdSetImpl);
    
    /// Borrowed
    /// 
    /// This module is responsible for safe, non-aliased, and concurrent access
    /// to table columns
    mod borrowed {
        use std::borrow::BorrowMut;
        use std::cell::UnsafeCell;
        use std::collections::HashMap;
        use std::fmt::Display;
        use std::marker::PhantomData;
        use std::ops::{Deref, DerefMut};
        use std::ptr::NonNull;
        use std::sync::atomic::{AtomicIsize, Ordering};

        use crate::EntityId;

        use super::{AnyPtr, Component, DbError};

        const NOT_BORROWED: isize = 0isize;
        const MUTABLE_BORROW: isize = -1isize;

        #[derive(Default, Debug)]
        pub struct BorrowSentinel(AtomicIsize);

        impl BorrowSentinel {
            fn new() -> Self {
                Default::default()
            }
        }

        impl Deref for BorrowSentinel {
            type Target = AtomicIsize;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        #[inline]
        fn is_mut_borrow(value: isize) -> bool {
            value < NOT_BORROWED
        } 

        pub struct BorrowRef<'b> { borrow: &'b BorrowSentinel }

        impl<'b> BorrowRef<'b> {
            pub fn new(borrow: &'b BorrowSentinel) -> Option<Self> {
                loop {
                    let cur = borrow.load(Ordering::SeqCst);
                    let new = cur + 1;
                    if is_mut_borrow(new) {
                        return None;
                    } else {
                        match borrow.compare_exchange(
                            cur, new, 
                            Ordering::SeqCst, 
                            Ordering::SeqCst
                        ) {
                            Ok(_) => {
                                return Some(BorrowRef { borrow });
                            },
                            Err(_) => {
                                continue; // someone else likely interacted with this borrow, try again
                            },
                        }
                    }
                }
            }
        }

        impl<'b> Drop for BorrowRef<'b> {
            fn drop(&mut self) {
                let borrow = self.borrow;
                #[cfg(debug_assertions)]
                {
                    let cur = borrow.load(Ordering::SeqCst);
                    debug_assert!(!is_mut_borrow(cur));
                }
                borrow.fetch_sub(1isize, Ordering::SeqCst);
            }
        }

        pub struct BorrowRefMut<'b> { borrow: &'b BorrowSentinel }

        impl<'b> BorrowRefMut<'b> {
            pub fn new(borrow: &'b BorrowSentinel) -> Option<Self> {
                let cur = NOT_BORROWED;
                let new = MUTABLE_BORROW;
                match borrow.compare_exchange(
                        cur, new, 
                        Ordering::SeqCst, 
                        Ordering::SeqCst
                    ) {
                    Ok(_) => {
                        return Some(BorrowRefMut { borrow });
                    },
                    Err(_) => {
                        return None
                    },
                }
            }
        }

        impl<'b> Drop for BorrowRefMut<'b> {
            fn drop(&mut self) {
                let borrow = self.borrow;
                #[cfg(debug_assertions)]
                {
                    let cur = borrow.load(Ordering::SeqCst);
                    debug_assert!(is_mut_borrow(cur));
                }
                borrow.fetch_add(1isize, Ordering::SeqCst);
            }
        }

        #[derive(Debug)]
        pub enum BorrowError {
            AlreadyBorrowed,
        }

        type ColumnType<C: Component> = Vec<C>;

        pub struct ColumnRef<'b, C> {
            column: NonNull<ColumnType<C>>,
            borrow: BorrowRef<'b>,
        }

        impl<'b, C> ColumnRef<'b, C> {
            pub fn new(column: NonNull<ColumnType<C>>, borrow: BorrowRef<'b>) -> Self {
                Self { column, borrow }
            }
        }

        impl<C> Deref for ColumnRef<'_, C> {
            type Target = ColumnType<C>;

            fn deref(&self) -> &Self::Target {
                // SAFETY
                // Safe to access because we hold a runtime checked borrow
                unsafe { self.column.as_ref() }
            }
        }

        impl<'b, C: 'b> IntoIterator for ColumnRef<'b, C> {
            type Item = &'b C;
            type IntoIter = ColumnIter<'b, C>;

            fn into_iter(self) -> Self::IntoIter {
                let size = self.len();
                ColumnIter {
                    column: unsafe { self.column.as_ref() },
                    borrow: self.borrow,
                    size,
                    next: 0usize,
                }
            }
        }
        
        pub struct ColumnIter<'b, C> {
            column: &'b ColumnType<C>,
            borrow: BorrowRef<'b>,
            size: usize,
            next: usize,
        }
        
        impl<'b, C: 'b> Iterator for ColumnIter<'b, C> {
            type Item = &'b C;
            
            fn next(&mut self) -> Option<Self::Item> {
                let val = self.column.get(self.next);
                self.next = std::cmp::min(self.next + 1, self.size);
                val
            }
        }

        pub struct ColumnRefMut<'b, T> {
            column: NonNull<Vec<T>>,
            borrow: BorrowRefMut<'b>,
        }

        impl<'b, T> ColumnRefMut<'b, T> {
            pub fn new(column: NonNull<Vec<T>>, borrow: BorrowRefMut<'b>) -> Self {
                Self { column, borrow }
            }
        }

        impl<T> Deref for ColumnRefMut<'_, T> {
            type Target = Vec<T>;

            fn deref(&self) -> &Self::Target {
                // SAFETY
                // Safe to access because we hold a runtime checked borrow
                unsafe { self.column.as_ref() }
            }
        }

        impl<T> DerefMut for ColumnRefMut<'_, T> {
            fn deref_mut(&mut self) -> &mut Self::Target {
                // SAFETY
                // Safe to access because we hold a runtime checked borrow
                unsafe { self.column.as_mut() }
            }
        }

        impl<'b, T: 'b> IntoIterator for ColumnRefMut<'b, T> {
            type Item = &'b mut T;
            type IntoIter = ColumnIterMut<'b, T>;

            fn into_iter(mut self) -> Self::IntoIter {
                let size = self.len();
                Self::IntoIter {
                    column: unsafe { self.column.as_mut() },
                    borrow: self.borrow,
                    size,
                    next: 0usize,
                    invariant: PhantomData::default()
                }
            }
        }

        pub struct ColumnIterMut<'b, T> {
            column: &'b mut Vec<T>,
            borrow: BorrowRefMut<'b>,
            size: usize,
            next: usize,
            invariant: PhantomData<&'b mut T>,
        }
    
        impl<'b, T: 'b> Iterator for ColumnIterMut<'b, T> {
            type Item = &'b mut T;

            fn next<'n>(&'n mut self) -> Option<Self::Item> {
                let val = self.column.get_mut(self.next);
                self.next = std::cmp::min(self.next + 1, self.size);

                // SAFETY
                // This is safe because we offer no other interface for accessing the items
                // we are iterating over, and we promise to only ever yield one mutable
                // reference to any given element, while upholding runtime borrow checks
                unsafe { std::mem::transmute::<Option<&mut T>, Option<&'b mut T>>(val) }
            }
        }

        const COLUMN_LENGTH_MAXIMUM: usize = 2^14;

        /// The owner of the actual data we are interested in. 
        #[derive(Default, Debug)]
        pub struct Column<C: Component> {
            /// INVARIANT: 
            /// 
            /// For an entity in a table, its associated components must
            /// always occupy the same index in each column. Failure to
            /// uphold this invariant will result in undefined behavior
            values: UnsafeCell<Vec<C>>,
            borrow: BorrowSentinel,
        }

        impl<C: Component> Display for Column<C> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "Column\n")?;
                write!(f, "\t{:?}\n", self.borrow)?;
                write!(f, "\t{:?}\n", unsafe { &*self.values.get() } )
            }
        }
        
        impl<'b, C: Component> Column<C> {
            pub fn new() -> Self {
                Self {
                    values: UnsafeCell::new(Vec::new()),
                    borrow: BorrowSentinel::new(),
                }
            }
            
            pub fn borrow_column(&'b self) -> ColumnRef<'b, C> {
                self.try_borrow().expect("column was already mutably borrowed")
            }
        
            fn try_borrow(&'b self) -> Result<ColumnRef<'b, C>, BorrowError> {
                match BorrowRef::new(&self.borrow) {
                    Some(borrow) => {
                        let column = unsafe {
                            NonNull::new_unchecked(self.values.get())
                        };
                        Ok(ColumnRef::new(column, borrow))
                    },
                    None => {
                        Err(BorrowError::AlreadyBorrowed)
                    },
                }
            }
            
            pub fn borrow_column_mut(&'b self) -> ColumnRefMut<'b, C> {
                self.try_borrow_mut().expect("column was already borrowed")
            }
            
            fn try_borrow_mut(&'b self) -> Result<ColumnRefMut<'b, C>, BorrowError> {
                match BorrowRefMut::new(&self.borrow) {
                    Some(borrow) => {
                        let column = unsafe {
                            NonNull::new_unchecked(self.values.get())
                        };
                        Ok(ColumnRefMut::new(column, borrow))
                    },
                    None => {
                        Err(BorrowError::AlreadyBorrowed)
                    },
                }
            }
            
            /// Moves a single component from one [Column] to another, if they are the same type
            pub fn dynamic_move(entity: &EntityId, index: usize, from: &AnyPtr, dest: &AnyPtr)
                -> Result<usize, DbError> {
                println!("DYNAMIC MOVE {}, {}", entity, index);
                
                let mut from = from.downcast_ref::<Column<C>>()
                    .ok_or(DbError::ColumnTypeDiscrepancy)?.borrow_column_mut();
                
                let mut dest = dest.downcast_ref::<Column<C>>()
                    .ok_or(DbError::ColumnTypeDiscrepancy)?.borrow_column_mut();
                
                debug_assert!(from.len() < index);
                
                let component = from.remove(index);
                dest.push(component);
                Ok(dest.len())
            }

            /// Resizes the column to hold at least [min_size] components
            pub fn dynamic_resize(column: &AnyPtr, min_size: usize) -> usize {
                println!("dynamic resize");

                let real_column = column
                    .downcast_ref::<Column<C>>()
                    .expect("column type mismatch in dynamic method call");

                real_column.resize_minimum(min_size)
            }
            
            /// Constructs a [Column] and returns a type erased pointer to it
            pub fn dynamic_ctor() -> AnyPtr {
                Box::new(Column::<C>::new())
            }

            /// Creates a new instance of the component type C at the specified index in the column
            pub fn dynamic_instance(column: &AnyPtr, index: usize) {
                // TODO: Just trying to get this working - need some guard rails around here
                let real_column = column
                    .downcast_ref::<Column<C>>()
                    .expect("column type mismatch in dynamic method call");

                let mut column = real_column.borrow_column_mut();
                column[index] = Default::default();
            }

            pub fn resize_minimum(&self, min_size: usize) -> usize {
                let power_of_two_index = std::cmp::max(min_size, 1).next_power_of_two();
                debug_assert!(power_of_two_index < COLUMN_LENGTH_MAXIMUM);

                println!("making room for {} items with {} spaces", min_size, power_of_two_index);
                
                let mut column = self.borrow_column_mut();
                let len = column.len();
                if min_size >= len {
                    column.resize_with(power_of_two_index, Default::default);
                }
                let new_len = column.len();
                new_len
            }

        }
    } // borrowed ======================================================================
    use borrowed::*;
    use transfer::TransferGraph;
    
    const COMMUTATIVE_HASH_TYPE_ZERO: CommutativeHashValue = 0 as CommutativeHashValue;
    const COMMUTATIVE_HASH_PRIME: CommutativeHashValue = 0x29233AAB26330D; // 11579208931619597

    /// [CommutativeId]
    #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    struct CommutativeId(CommutativeHashValue);
    const COMMUTATIVE_ID_INIT: CommutativeId = CommutativeId(COMMUTATIVE_HASH_PRIME);

    impl CommutativeId {
        pub fn and(&self, other: &CommutativeId) -> Self {
            Self::combine(self, other)
        }

        fn combine(first: &Self, other: &Self) -> Self {
            debug_assert!(first.non_zero());
            debug_assert!(other.non_zero());

            CommutativeId(
                first.0
                    .wrapping_add(other.0)
                    .wrapping_add(other.0
                        .wrapping_mul(first.0))
            )
        }

        #[inline(always)]
        fn non_zero(&self) -> bool {
            !(self.0 == 0)
        }
    }

    impl FromIterator<CommutativeId> for CommutativeId {
        fn from_iter<T: IntoIterator<Item = CommutativeId>>(iter: T) -> Self {
            iter.into_iter().fold(COMMUTATIVE_ID_INIT, |acc, x| {
                CommutativeId::combine(&acc, &x)
            })
        }
    }

    impl FromIterator<CommutativeHashValue> for CommutativeId {
        fn from_iter<T: IntoIterator<Item = CommutativeHashValue>>(iter: T) -> Self {
            iter.into_iter().fold(COMMUTATIVE_ID_INIT, |acc, x| {
                CommutativeId::combine(&acc, &CommutativeId(x))
            })
        }
    }

    impl<'i> FromIterator<&'i CommutativeHashValue> for CommutativeId {
        fn from_iter<T: IntoIterator<Item = &'i CommutativeHashValue>>(iter: T) -> Self {
            iter.into_iter().fold(COMMUTATIVE_ID_INIT, |acc, x| {
                CommutativeId::combine(&acc, &CommutativeId(*x))
            })
        }
    }

    #[test]
    fn test_commutative_id() {
        let iter_a = [StableTypeId::of::<f32>(), StableTypeId::of::<f64>()];
        let iter_aa = [StableTypeId::of::<f64>(), StableTypeId::of::<f32>()];
        let iter_b = [StableTypeId::of::<i8>(), StableTypeId::of::<i16>()];
        let iter_c = [StableTypeId::of::<i8>(), StableTypeId::of::<i16>(), StableTypeId::of::<i32>()];
        let id_a = CommutativeId::from_iter(iter_a.into_iter().map(|i| i.0));
        let id_aa = CommutativeId::from_iter(iter_aa.into_iter().map(|i| i.0));
        let id_b = CommutativeId::from_iter(iter_b.into_iter().map(|i| i.0));
        let id_c = CommutativeId::from_iter(iter_c.into_iter().map(|i| i.0));
        
        assert_ne!(id_a, id_b);
        assert_ne!(id_a, id_c);
        assert_ne!(id_b, id_c);

        assert_eq!(id_a, id_aa)
    }

    pub trait Component: Default + Debug + 'static {}
    
    /// Component implementation for the unit type
    /// Every entity automatically gets this component upon creation
    impl Component for () {}

    /// Type erased entry into a table which describes a single column
    /// A column contains one type of component. An single index into a table describes 
    /// an entity made up of different components in different columns
    pub struct TableEntry {
        pub tyid: StableTypeId, // The type id of Column<T>  ([Self::data])
        pub data: AnyPtr, // Column<T>
        
        fn_constructor: fn() -> AnyPtr,
        fn_instance: fn(&AnyPtr, usize),
        fn_move: fn(&EntityId, usize, &AnyPtr, &AnyPtr) -> Result<usize, DbError>,
        fn_resize: fn(&AnyPtr, usize) -> usize,
    }
    
    impl<'b> TableEntry {
        pub fn resize_minimum(&self, min_size: usize) -> usize {
            (self.fn_resize)(&self.data, min_size)
        }

        pub fn iter<T: Component>(&'b self) -> borrowed::ColumnIter<'b, T> {
            debug_assert!(StableTypeId::of::<T>() == self.tyid);

            let column = self.data.downcast_ref::<Column<T>>()
                .expect("expected matching column types");
            let column_ref = column.borrow_column();
            let column_iter = column_ref.into_iter();

            column_iter
        }
        
        pub fn iter_mut<T: Component>(&'b self) -> borrowed::ColumnIterMut<'b, T> {
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
        columns: RwLock<HashMap<CType, TableEntry>>, // hashmap never changes after the table is fully initialized
        entitym: RwLock<HashMap<EntityId, usize>>,
        numfree: AtomicUsize,
        free: Mutex<Vec<usize>>,
        size: AtomicUsize, // this should be equal to entitym.len()
    }
    
    impl Table {
        fn new(family: FamilyId) -> Self {
            Table {
                family: family,
                columns: RwLock::new(HashMap::new()),
                entitym: RwLock::new(HashMap::new()),
                free: Mutex::new(Vec::new()),
                numfree: AtomicUsize::new(0),
                size: AtomicUsize::new(0),
            }
        }

        /// Gets the index of the next free row in the table
        /// expanding the table if necessary
        fn get_next_free_row(&self) -> usize {
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
        fn set_component<C: Component>(
            &self,
            entity: &EntityId,
            component: C
        ) -> Result<(), DbError>
        {
            println!("setting component");

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
            
            println!("got next index: {}", index);

            match column_guard {
                Some(column_guard) => {
                    println!("using provided column lock guard");

                    column_guard.iter().for_each(|(_, table_entry)| {
                        let column = &table_entry.data;
                        (table_entry.fn_instance)(column, index)
                    });
                },
                None => {
                    println!("attempting to acquire column lock");
                    
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
        const fn of<C: Component>() -> Self {
            Self(StableTypeId::of::<C>())
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

    #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub struct FamilyId(CommutativeId);

    impl Display for FamilyId {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0.0)
        }
    }

    impl<'i> FromIterator<&'i ComponentType> for FamilyId {
        fn from_iter<T: IntoIterator<Item = &'i ComponentType>>(iter: T) -> Self {
            FamilyId(CommutativeId::from_iter(iter.into_iter().map(|id| id.0.0)))
        }
    }

    /// An immutable set of family id's
    #[derive(Clone)]
    pub struct FamilyIdSet {
        ptr: Arc<FamilyIdSetInner>, // thread-local?
    }

    impl FamilyIdSet {
        pub fn contains(&self, id: &FamilyId) -> bool {
            self.ptr.1.contains(id)
        }

        pub fn iter(&self) -> impl Iterator<Item = &FamilyId> {
            self.ptr.1.iter()
        }
    }

    impl<'i> FromIterator<FamilyId> for FamilyIdSet {
        fn from_iter<I: IntoIterator<Item = FamilyId>>(iter: I) -> Self {
            let set: FamilyIdSetImpl = iter.into_iter().collect();
            let set_id = CommutativeId::from_iter(set.iter().map(|id| id.0));
            FamilyIdSet { ptr: Arc::new((set_id, set)) }
        }
    }

    impl<'i, I> From<I> for FamilyIdSet
    where
        I: IntoIterator<Item = &'i FamilyId>,
    {
        fn from(into_iter: I) -> Self {
            FamilyIdSet::from_iter(into_iter.into_iter().cloned())
        }
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
                    write!(f, "table doesn't exist for the given family id")
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

        // The actual database tables are stored here. One table per family
        // Each table is comprised of a set of type erased columns
        // All access should be via the read side of the RwLock EXCEPT for
        // creating new tables. Synchronization is carefully handled on a much
        // more granular level for entity interactions.
        tables: Arc<RwLock<HashMap<FamilyId, Table>>>,

        // Data mappings and caches. Stores some critical information for
        // quickly querying the DB
        maps: DbMaps, // cache?
    }

    impl EntityDatabase {
        /// Creates a new [EntityDatabase]
        pub fn new() -> Self {
            let db = Self {
                allocator: EntityAllocator::new(),
                tables: Arc::new(RwLock::new(HashMap::new())),
                maps: DbMaps::new(),
            };

            // prettier debug output when dealing with unit/null components
            StableTypeId::register_debug_info::<()>();

            // setup the unit/null component family
            let unit_family_set = ComponentTypeSet::from_iter([ComponentType::of::<()>()]);
            let family_id = db.new_family(unit_family_set).expect("please report this bug - unable to create unit component family");

            println!("setup debug/null family: {:?}", family_id);

            db
        }

        /// Creates an entity, returning its [EntityId]
        pub fn create(&self) -> Result<EntityId, CreateEntityError> {
            println!("creating entity");

            let entity = self.allocator.alloc().map_err(|_| {
                CreateEntityError::IdAllocatorError
            })?;

            let unit_family_set = ComponentTypeSet::from_iter([ComponentType::of::<()>()]);
            let unit_family_id = self.maps.get::<(ComponentTypeSet, FamilyId)>(&unit_family_set)
                .ok_or(CreateEntityError::DbError(DbError::FailedToFindFamilyForSet(unit_family_set)))?;
            
            // add the entity to the unit/null family
            {
                let mut guard = self.maps
                    .mut_map::<(EntityId, FamilyId)>()
                    .ok_or(DbError::FailedToAcquireMapping)
                    .map_err(|err| CreateEntityError::DbError(err))?;
        
                guard.insert(entity, unit_family_id);

                match self.tables.read() {
                    Ok(guard) => {
                        let table = guard
                            .get(&unit_family_id)
                            .ok_or(DbError::FamilyDoesntExist(unit_family_id))?;
                        
                        table.create_instance(&entity, None, None)?;
                    },
                    Err(e) => {
                        println!("failed to create entity: {}", e);
                    }
                }
            }

            

            Ok(entity)
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

            println!("adding component of type {} to {:?}", cty, entity);
            
            let cur_family = self.maps.get::<(EntityId, FamilyId)>(&entity);
            println!("  {:?} currently belongs to family {:?}", entity, cur_family);
            
            let new_family = self.find_new_family(&entity, &delta)?;
            if new_family == cur_family.unwrap() {
                println!("  no need to move {:?} as it already belongs to {:?}", entity, new_family);
            } else {
                println!("  moving {:?} from {:?} to {:?}", entity, cur_family, new_family);
            }

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
                    println!("family already contains {}", component);
                    return Ok(*current)
                } else {
                    let new_components_iter =
                        components
                        .iter()
                        .cloned()
                        .chain([*component]);

                    let new_components = ComponentTypeSet::from_iter(new_components_iter);

                    println!("new component set names: {}", new_components.names());
                    println!("new component type set id: {}", new_components.id);

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
            components: ComponentTypeSet
        ) -> Result<FamilyId, DbError> {
            println!("creating new family for component set {}", components.names());
            
            let family_id = FamilyId::from_iter(components.iter());

            println!("new family id: {:?}", family_id);

            
            {
                let mut guard = self.tables.write()
                    .map_err(|e| DbError::UnableToAcquireTablesLock(e.to_string()))?;
                
                let table = Table::new(family_id);
                guard.insert(family_id, table);
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
            println!("resolve entity transfer");

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
                    let table_guard;
                    let mut entity_map_from_guard;
                    let mut entity_map_dest_guard;
                    let data_table_from;
                    let data_table_dest;
                    let column_from_guard;
                    let column_dest_guard;
                    
                    if let Ok(tab_guard) = self.tables.try_read() {
                        table_guard = tab_guard;
                    } else {
                        continue; // locking has failed, loop again
                    }

                    data_table_from = table_guard
                        .get(&curr_family)
                        .ok_or(DbError::TableDoesntExistForFamily(curr_family))?;

                    if let Ok(col_from_guard) = data_table_from.columns.try_read() {
                        column_from_guard = col_from_guard;
                    } else {
                        continue;
                    }

                    data_table_dest = table_guard
                        .get(&dest_family)
                        .ok_or(DbError::TableDoesntExistForFamily(dest_family))?;

                    if let Ok(col_dest_guard) = data_table_dest.columns.try_read() {
                        column_dest_guard = col_dest_guard;
                    } else {
                        continue;
                    }

                    if let Ok(map_from_guard) = data_table_from.entitym.try_write() {
                        entity_map_from_guard = map_from_guard;
                    } else {
                        continue;
                    }

                    if let Ok(map_dest_guard) = data_table_dest.entitym.try_write() {
                        entity_map_dest_guard = map_dest_guard;
                    } else {
                        continue;
                    }

                    println!("acquired locks to resolve entity transfer");

                    // If we've successfully locked everything we need this loop, we can proceeed
                    //if let Some(_) = entity_map_from_guard.remove(entity) {
                        entity_map_from_guard.remove(entity);
                        println!("removed entity from old table map");
                        
                        // The entity should exist in this table, as it has an index,
                        // we've removed its mapping in its old table, now lets move
                        // its data and then patch up the new tables mapping, as well as
                        // any other dbmaps we might be concerned about
                            
                        let new_entity_index = data_table_dest.get_next_free_row();

                        // Move the entity from one table to the other, column by column.
                        // This data is type-erased so we use a special "move" function pointer
                        // we created with each column that knows what to do. We have to tell the
                        // column in which to store the entity, however, as it's decided by the
                        // table itself
                        for (component_type, table_entry_from) in column_from_guard.iter() {
                            println!("QQQ");
                            println!("checking table for matching column...");

                            if let Some(table_entry_dest) = column_dest_guard.get(component_type) {
                                println!("moving component");
                                let index = (table_entry_from.fn_move)(entity, new_entity_index, &table_entry_from.data, &table_entry_dest.data)?;
                            } else {
                                println!("create instance");
                                data_table_dest.create_instance(entity, Some(new_entity_index), Some(&column_dest_guard))?;
                            }
                        }
                        println!("||| setting new entity index");
                        entity_map_dest_guard.insert(*entity, new_entity_index);
                        println!("\nEMFG:::{:?}", entity_map_from_guard);
                        println!("EMDG:::{:?}", entity_map_dest_guard);
                    //}
                    
                    println!("patching db maps for entity transfer");
                    // patch db maps
                    // guard scope
                    {
                        let mut guard = self.maps
                            .mut_map::<(EntityId, FamilyId)>()
                            .ok_or(DbError::FailedToAcquireMapping)?;
                        guard.insert(*entity, dest_family);
                    }
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
                let table_guard = self.tables.read().expect("unable to read data tables");
                let table: &Table = table_guard.get(&family)
                    .ok_or(DbError::TableDoesntExistForFamily(family))?;

                println!("set component for");
                println!("{}", &self);
                
                table.set_component(entity, component)?;
            }
            Ok(())
        }
    }

    impl Display for EntityDatabase {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "db dump\n")?;
            {
                if let Ok(guard) = self.tables.read() {
                    for item in guard.iter() {
                        write!(f, "{}",item.1)?;
                    }
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

    pub mod conflict {
        use std::{collections::{HashSet, HashMap, hash_map::IntoValues}, cell::{Cell, RefCell}, marker::PhantomData, ops::{Deref, DerefMut}};

        pub trait Dependent {
            type Dependency: PartialEq + Eq + std::hash::Hash;
            
            // The 'iter lifetime associated with these two functions deserves to
            // be reviewed. The only reason this trait is structured this way is
            // to avoid polluting the code with additional lifetimes. The resulting
            // compromise is that [Self::Dependency]'s yielded by the returned
            // iterators must be copied or cloned, they cannot yield references
            fn dependencies<'iter>(&'iter self) -> impl Iterator<Item = Self::Dependency> + 'iter;
            fn exclusive_dependencies<'iter>(&'iter self) -> impl Iterator<Item = Self::Dependency> + 'iter {
                std::iter::empty()
            }
        }
        
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct ConflictColor(usize);
        impl ConflictColor {
            fn as_usize(&self) -> usize {
                self.0
            }
        }

        #[derive(Debug)]
        pub struct ConflictGraphNode<K, V, D> {
            key: K,
            val: V,
            dep: HashSet<D>,
            exc: HashSet<D>,
            edges: RefCell<HashSet<usize>>,
            color: Cell<Option<ConflictColor>>,
        }

        #[derive(Debug)]
        pub struct Init<K, V, D>(Vec<ConflictGraphNode<K, V, D>>);

        impl<K, V, D> Init<K, V, D> {
            fn new() -> Self {
                Self(Vec::new())
            }
        }

        impl<K, V, D> Deref for Init<K, V, D> {
            type Target = Vec<ConflictGraphNode<K, V, D>>;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl<K, V, D> DerefMut for Init<K, V, D> {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }

        type Buckets<K, V> = HashMap<usize, Vec<(K, V)>>;

        #[derive(Debug)]
        pub struct Built<K, V> {
            buckets: Buckets<K, V>
        }

        /// A [ConflictGraph] is an expensive structure to build, and once it's built
        /// it is considered entirely immutable, in addition, the data needed to
        /// build the graph and the data needed to query a built graph is different.
        /// Because of these properties, we use type-state to encode an initializing
        /// state and a built state directly into the type system, with different
        /// exposed methods and different internal representations on the two
        pub trait ConflictGraphState {}
        impl<K, V, D> ConflictGraphState for Init<K, V, D> {}
        impl<K, V> ConflictGraphState for Built<K, V> {}

        const COLOR_ZERO: ConflictColor = ConflictColor(0);
        
        #[derive(Debug)]
        pub struct ConflictGraph<K, V: Dependent, State: ConflictGraphState = Init<K, V, <V as Dependent>::Dependency>> {
            state: State,
            _k: PhantomData<K>,
            _v: PhantomData<V>,
        }

        impl<'g, K, V> ConflictGraph<K, V, Init<K, V, <V as Dependent>::Dependency>>
        where
            V: Dependent,
            K: Clone + PartialEq + Eq,
            K: std::hash::Hash,
        {
            pub fn new() -> Self {
                Self {
                    state: Init::new(),
                    _k: PhantomData::default(),
                    _v: PhantomData::default(),
                }
            }

            pub fn insert(&mut self, key: K, val: V) {
                let dep: HashSet<V::Dependency> = val.dependencies()
                                                     .collect();
                
                let exc: HashSet<V::Dependency> = val.exclusive_dependencies()
                                                     .collect();
                
                self.state.push(ConflictGraphNode {
                    key,
                    val,
                    dep,
                    exc,
                    edges: RefCell::new(HashSet::new()),
                    color: Cell::new(None),
                });

                self.resolve_edges()                
            }
            
            /// Create edges where nodes have exclusive dependencies which conflict
            /// with other nodes dependencies or exclusive dependencies
            fn resolve_edges(&mut self) {
                for (first_index, first)
                in self.state.iter().enumerate() {

                    for (other_index, other)
                    in self.state.iter().enumerate() {

                        // If we are pointing at ourself, go to the next iteration
                        if std::ptr::eq(first, other) { continue; }
                        
                        for exc in first.exc.iter() {
                            // Chained iterator iterates all of the dependencies and
                            // exclusive dependencies of our other node, we compare
                            // each iterated item with the current exclusive
                            // dependency of the current iterated node
                            for dep 
                            in other.dep.iter()
                                        .chain(other.exc.iter()) {
                                if exc == dep {
                                    // We are using a hashset to store edges, thus
                                    // duplicates are handled implicitly and we ignore
                                    first.edges.borrow_mut().insert(other_index);
                                    other.edges.borrow_mut().insert(first_index);
                                }
                            }
                        }
                    }
                }
            }

            fn color(&mut self) {
                let nodes = &self.state;
                let mut uncolored = nodes.len();
                let mut palette = Vec::from([COLOR_ZERO]);

                while let Some(node) = self.pick_next() {
                    let mut available_colors = palette.clone();

                    for neighbor_color 
                    in node.edges.borrow()
                                 .iter()
                                 .filter_map(|i|
                                    nodes.get(*i)
                                         .and_then(|n| 
                                            n.color.get())) {
                        
                        if let Some(pos) = available_colors.iter()
                                                                  .position(|c| *c == neighbor_color) {
                            available_colors.remove(pos);
                        }
                    }

                    if let Some(color) = available_colors.first() {
                        node.color.set(Some(*color));
                    } else {
                        palette.push(ConflictColor(palette.len()));
                        node.color.set(Some(*palette.last().expect("expected color")));
                    }

                    uncolored -= 1;
                }
                debug_assert!(uncolored == 0);
            }
            
            fn pick_next(&self) -> Option<&ConflictGraphNode<K, V, <V as Dependent>::Dependency>> {
                let nodes = &self.state;
                let mut candidate: Option<&ConflictGraphNode<K, V, <V as Dependent>::Dependency>> = None;
                let (mut candidate_colored, mut candidate_uncolored) = (0, 0);
                
                for node in nodes.iter() {
                    
                    // skip already colored nodes
                    if node.color.get().is_some() { continue; }
                    
                    // sums colored and uncolored neighbors for this node
                    let (colored, uncolored) = node.edges
                            .borrow()
                            .iter()
                            .filter_map(|i| nodes.get(*i))
                            .fold((0, 0), |(mut c, mut u), x| {
                                x.color.get()
                                    .and_then(|v| { c += 1; Some(v) })
                                    .or_else(|| { u += 1; None });
                                (c, u) 
                            });
                    
                    if (colored > candidate_colored) || ((colored == candidate_colored) && (uncolored > candidate_uncolored)) {
                        // this is our new candidate
                        candidate_colored = colored;
                        candidate_uncolored = uncolored;
                        candidate = Some(node);
                        continue;
                    } else {
                        // if we have no candidate at all, pick this one
                        if candidate.is_none() {
                            candidate = Some(node);
                        }
                        continue;
                    }
                }
                candidate
            }

            pub fn build(mut self) -> ConflictGraph<K, V, Built<K, V>> {
                self.color();

                let mut buckets = HashMap::new();
                
                // destructively iterate the state and fill up our buckets
                for node in self.state.0.into_iter() {
                    let color = node.color.get().expect("please report this bug - all nodes must be colored");
                    
                    // we're wrapping kv in an Option to use the take().unwrap()
                    // methods to bypass a deficiency with borrow-checking in the
                    // entry API
                    let mut kv = Some((node.key, node.val));

                    buckets.entry(color.as_usize())
                           .and_modify(|e: &mut Vec<(K, V)>| e.push(kv.take().unwrap()))
                           .or_insert_with(|| vec![kv.take().unwrap()]);
                }

                ConflictGraph {
                    state: Built {
                        buckets
                    },
                    _k: PhantomData,
                    _v: PhantomData,
                }
            }
        }

        impl<'g, K, V> ConflictGraph<K, V, Built<K, V>>
        where
            V: Dependent
        {
            /// Returns an iterator over separate collections of non-conflicting items
            pub fn iter(&self) -> impl Iterator<Item = &Vec<(K, V)>> {
                self.state.buckets.values()
            }
        }
        
        impl<K, V> IntoIterator for ConflictGraph<K, V, Built<K, V>>
        where
            V: Dependent
        {
            type Item = Vec<(K, V)>;
            type IntoIter = IntoValues<usize, Vec<(K, V)>>;

            fn into_iter(self) -> Self::IntoIter {
                self.state.buckets.into_values()
            }
        }

        pub struct ConflictGraphIter<'g, K, T> {
            colors: &'g HashMap<K, T>,
        }

        impl<'g, K, T> Iterator for ConflictGraphIter<'g, K, T>
        where
            T: 'g,
            K: 'g,
        {
            type Item = &'g [(K, T)];

            fn next(&mut self) -> Option<Self::Item> {
                todo!()
            }
        }
        
        #[cfg(test)]
        mod test {
            use super::{Dependent, ConflictGraph};

            #[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
            enum Dep {
                Blue,
                Yellow,
                Green,
                Cyan,
                White,
            }

            #[derive(Debug)]
            struct Consumer {
                r: Vec<Dep>,
                w: Vec<Dep>,
            }
            impl Dependent for Consumer {
                type Dependency = Dep;

                fn dependencies(&self) -> impl Iterator<Item = Self::Dependency> {
                    self.r.clone().into_iter()
                }

                fn exclusive_dependencies(&self) -> impl Iterator<Item = Self::Dependency> {
                    self.w.clone().into_iter()
                }
            }
            
            #[test]
            fn build_conflict_graph() {
                let resources = [
                    Consumer { r: vec![Dep::Cyan], w: vec![Dep::Blue] },
                    Consumer { r: vec![Dep::Cyan], w: vec![Dep::Yellow] },
                    Consumer { r: vec![Dep::Cyan], w: vec![Dep::Green] },
                    Consumer { r: vec![Dep::Cyan, Dep::White], w: vec![Dep::Blue] },
                    
                    Consumer { r: vec![], w: vec![Dep::White] },
                    Consumer { r: vec![], w: vec![Dep::Yellow] },

                    Consumer { r: vec![Dep::Cyan], w: vec![] },
                    Consumer { r: vec![Dep::Blue], w: vec![] },
                    Consumer { r: vec![Dep::Cyan, Dep::Blue], w: vec![] },
                    Consumer { r: vec![Dep::Blue, Dep::Green], w: vec![] },
                    
                    Consumer { r: vec![Dep::Green], w: vec![] },
                    Consumer { r: vec![Dep::Yellow], w: vec![] },
                ];

                let mut graph = ConflictGraph::new();
                
                for (index, resource) in resources.into_iter().enumerate() {
                    graph.insert(index, resource);
                }

                let conflict_free = graph.build();
                
                for (index, bucket) in conflict_free.iter().enumerate() {
                    println!("bucket: {}", index);
                    for (i, consumer) in bucket {
                        let s_writes = format!("*{:?}*", consumer.w);
                        println!("\t\t{:3}:{:24}{:?}", i, s_writes, consumer.r);
                    }
                }
            }
        }
    } // conflict ======================================================================

    /// Transformations
    /// 
    /// Functionality related to transforming data contained in an
    /// [crate::database::reckoning::EntityDatabase]
    pub mod transform {
        use std::collections::HashMap;
        use std::hash::Hash;
        use std::marker::PhantomData;
        use crate::database::ComponentType;
        use crate::database::ConflictGraph;
        use crate::database::Dependent;

        use super::FamilyId;
        use super::FamilyIdSet;
        use super::Component;
        use super::EntityDatabase;

        pub struct Phase {
            subphases: Vec<HashMap<TransformationId, DynTransform>>,
        }

        impl Phase {
            pub fn new() -> Self {
                Phase {
                    subphases: Vec::new()
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
                let transforms: Vec<(TransformationId, DynTransform)> = self.subphases
                    .drain(..)
                    .flatten()
                    .chain([transform_tuple].into_iter())
                    .collect();

                let mut graph = ConflictGraph::new();
                
                transforms.into_iter()
                          .for_each(|(k, v)| graph.insert(k, v));
                
                let deconflicted = graph.build();
                
                deconflicted.into_iter().for_each(|bucket| {
                    let subphase: HashMap<TransformationId, DynTransform> = HashMap::from_iter(bucket.into_iter());
                    self.subphases.push(subphase);
                });
            }

            pub fn run_on(&mut self, _db: &EntityDatabase) -> PhaseResult {
                for _ in self.subphases.iter_mut() {
                    todo!()
                    // queue jobs one subphase at a time
                }
                Ok(())
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct TransformationId(std::any::TypeId);
        
        pub trait Transformation: 'static {
            type Data: Selection;
            fn run(data: Rows<Self::Data>) -> TransformationResult;
            fn messages(_: Messages) { todo!() }
            
            /// Returns a unique identifier for a given transformation impl
            fn id() -> TransformationId where Self: 'static {
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

            fn dependencies<'iter>(&'iter self) -> impl Iterator<Item = Self::Dependency> + 'iter {
                self.reads.iter().cloned()
            }

            fn exclusive_dependencies<'iter>(&'iter self) -> impl Iterator<Item = Self::Dependency> + 'iter {
                self.writes.iter().cloned()
            }
        }

        pub struct Read<C: Component> { marker: PhantomData<C>, }
        pub struct Write<C: Component> { marker: PhantomData<C>, }
        pub trait ReadWrite {}
        impl<C> ReadWrite for Read<C> where C: Component {}
        impl<C> ReadWrite for Write<C> where C: Component {}
        pub trait MetaData {}
        impl<'db, C> MetaData for Read<C> where C: Component {}
        impl<'db, C> MetaData for Write<C> where C: Component {}
        
        pub struct RowIter<'db, RTuple> {
            pub(crate) db: &'db EntityDatabase,
            pub(crate) family: FamilyId,
            pub(crate) marker: PhantomData<RTuple>,
        }

        impl<'db, RTuple> RowIter<'db, RTuple> {
            pub fn new(db: &'db EntityDatabase, family: FamilyId) -> Self {
                Self { db, family, marker: PhantomData::default() }
            }
        }
        
        #[const_trait]
        pub trait SelectOne<'db> {
            type Ref;
            type Type;
            fn reads() -> Option<ComponentType> { None }
            fn writes() -> Option<ComponentType> { None }
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

        pub trait Selection {
            fn as_rows<'db, T>(db: &'db EntityDatabase) -> Rows<'db, T> {
                
                #![allow(unreachable_code, unused_variables)] todo!("actually get a family id set");

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
            pub fn database(&self) -> &'db EntityDatabase { self.db }
            pub fn families(&self) -> FamilyIdSet { self.fs.clone() }
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
    } // transformations ===============================================================
    
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

                impl<'a, $($t,)+> crate::database::Selection for ($($t,)+)
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
pub use reckoning::transform;
pub use reckoning::transform::Rows;
pub use reckoning::transform::MetaData;
pub use reckoning::transform::SelectOne;
pub use reckoning::transform::Selection;
pub use reckoning::transform::RowIter;
pub use reckoning::conflict;
pub use reckoning::conflict::ConflictGraph;
pub use reckoning::conflict::ConflictColor;
pub use reckoning::conflict::Dependent;
pub use reckoning::Component;
pub use reckoning::ComponentType;
pub use reckoning::EntityDatabase;

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
    use std::default;

    use super::reckoning::*;
    use super::reckoning::transform::*;

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
