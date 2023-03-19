use std::hash::Hash;
use std::ops::Deref;
use std::sync::RwLock;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::fmt::Debug;


#[derive(Copy, Clone)]
pub union IdUnion {
    /// Interprets the id as (id, generation, _, flags)
    pub(crate) generational: (u32, u16, u8, u8),

    /// Interprets the id as (id, id) for relational links
    pub(crate) relational: (u32, u32),

    /// Interprets the id as a raw u64, also forces the union into 8 byte alignment
    pub(crate) aligned: u64,

    #[allow(dead_code)]
    /// Interprets the id as raw bytes
    pub(crate) bytes: [u8; 8],
}

/// An `EntityId` uniquely identifies a single entity
/// 
/// Consuming code uses `EntityId` instances to reference entities in the `EntityDatabase`
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct EntityId(pub(crate) IdUnion);

/// A `ComponentId` uniquely identifies a single component within a column of components
/// 
/// These are functionally the same as `EntityId`'s
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct ComponentId(pub(crate) IdUnion);

/// A `FamilyId` uniquely identifies a single family
/// 
/// These are functionally the same as `EntityId`'s
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[deprecated]
pub struct FamilyId(pub(crate) IdUnion);

/// A stable `TypeId` which (should) be common across builds. This isn't a guarantee, especially
/// across different rust versions. Used to generate better errors
/// 
/// StableTypeId's are simply a compile time FNV-1a hash of the type name provided by `std::any::type_name()`
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StableTypeId(pub(crate) u64);

// Impl's

// `IdUnion` unsafe is required to access the unions fields
impl PartialEq for IdUnion {
    fn eq(&self, other: &Self) -> bool {
        unsafe { PartialEq::eq(&self.aligned, &other.aligned) }
    }
}

impl Eq for IdUnion {}

impl PartialOrd for IdUnion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        unsafe { PartialOrd::partial_cmp(&self.aligned, &other.aligned) }
    }
}

impl Ord for IdUnion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        unsafe { Ord::cmp(&self.aligned, &other.aligned) }
    }
}

impl Hash for IdUnion {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        unsafe { Hash::hash(&self.aligned, state) }
    }
}

impl Debug for IdUnion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe { write!(f, "aligned [{}]", self.aligned) }
    }
}

// `EntityId`
impl EntityId {
    pub(crate) fn next_generation(&self) -> Self {
        unsafe {
            EntityId(IdUnion { generational: (
                self.generational.0,
                self.generational.1.wrapping_add(1),
                self.generational.2,
                self.generational.3,
            )})
        }
    }
    
    pub(crate) fn generational(idx: u32, gen: u16, m1: u8, m2: u8) -> Self {
        EntityId(IdUnion{generational: (idx, gen, m1, m2)})
    }
}

impl Deref for EntityId {
    type Target = IdUnion;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// `ComponentId`
impl From<EntityId> for ComponentId {
    fn from(value: EntityId) -> Self {
        ComponentId(value.0)
    }
}

impl From<EntityId> for FamilyId {
    fn from(value: EntityId) -> Self {
        FamilyId(value.0)
    }
}


static STABLE_TYPE_ID_NAME_MAP: LazyLock<RwLock<HashMap<StableTypeId, &'static str>>>
    = LazyLock::new(|| RwLock::new(HashMap::new()));

// `StableTypeId`
impl StableTypeId {
    fn name(&self) -> Option<&'static str> {
        match STABLE_TYPE_ID_NAME_MAP.read() {
            Ok(guard) => guard.get(self).copied(),
            Err(err) => panic!("poisoned mutex accessing stable type name: {}", err),
        }
    }
    
    const fn type_name<T>() -> &'static str where T: ?Sized + 'static {
        std::any::type_name::<T>()
    }

    const fn hash<T>() -> u64 where T: ?Sized + 'static {
        let name = Self::type_name::<T>();
        const_fnv1a_hash::fnv1a_hash_str_64(name)
    }

    pub const fn of<T>() -> Self where T: ?Sized + 'static {
        let hash = Self::hash::<T>();
        debug_assert!(hash & 0xFFFF != 0);
        StableTypeId(hash)
    }

    /// Registers name information for this type at runtime for richer debug messages
    /// 
    /// This function associates a `StableTypeId` with an intrinsic type name at runtime, it is necessary to
    /// call `register_debug_info` *before* any calls to `name` in order to get the types name
    pub fn register_debug_info<T>() where T: ?Sized + 'static {
        let name = Self::type_name::<T>();
        let tyid = Self::of::<T>();

        match STABLE_TYPE_ID_NAME_MAP.write() {
            Ok(mut guard) => guard.insert(tyid, name),
            Err(err) => panic!("poisoned mutex registering stable type debug info: {}", err),
        };
    }
}

impl Debug for StableTypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match Self::name(&self) {
            Some(name) => write!(f, "[{}]", name),
            None => write!(f, "[unknown type]"),
        }
    }
}
