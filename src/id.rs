use std::hash::Hash;
use std::ops::Deref;
use std::fmt::{Display, self};
use std::sync::{RwLock, Arc};
use std::collections::HashMap;
use std::sync::LazyLock;
use std::fmt::Debug;

use dashmap::DashSet;

use crate::database::ComponentType;


#[derive(Copy, Clone)]
pub union IdUnion {
    /// Interprets the id as (id, generation, _, flags)
    pub(crate) generational: (u32, u16, u8, u8),

    /// Interprets the id as (id, id) for relational links
    #[allow(dead_code)]
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

impl EntityId {
    pub fn inner(&self) -> IdUnion {
        self.0
    }
}

/// A `ComponentId` uniquely identifies a single component within a column of components
/// 
/// These are functionally the same as `EntityId`'s
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct ComponentId(pub(crate) IdUnion);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FamilyId(CommutativeId);

impl Display for FamilyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.0)
    }
}

impl<'i> FromIterator<&'i ComponentType> for FamilyId {
    fn from_iter<T: IntoIterator<Item = &'i ComponentType>>(iter: T) -> Self {
        FamilyId(CommutativeId::from_iter(iter.into_iter().map(|id| id.inner().raw_id())))
    }
}

/// An immutable set of family id's

pub(crate) type FamilyIdSetImpl = DashSet<FamilyId>; // INVARIANT: This type MUST NOT accept duplicates 
pub(crate) type FamilyIdSetInner = (CommutativeId, FamilyIdSetImpl);

#[derive(Debug, Clone)]
pub struct FamilyIdSet {
    ptr: Arc<FamilyIdSetInner>, // thread-local?
}

impl FamilyIdSet {
    pub fn contains(&self, id: &FamilyId) -> bool {
        self.ptr.1.contains(id)
    }

    pub fn insert(&self, family_id: FamilyId) {
        self.ptr.1.insert(family_id);
    }

    pub fn clone_into_vec(&self) -> Vec<FamilyId> {
        let mut collection = Vec::new();
        self.ptr.1.iter().for_each(|id| {
            collection.push(id.clone());
        });
        collection
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
}

impl Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "E[{:?}]", self.0)
    }
}

impl Deref for EntityId {
    type Target = IdUnion;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

static STABLE_TYPE_ID_NAME_MAP: LazyLock<RwLock<HashMap<StableTypeId, &'static str>>>
    = LazyLock::new(|| RwLock::new(HashMap::new()));

// `StableTypeId`
impl StableTypeId {
    pub fn name(&self) -> Option<&'static str> {
        match STABLE_TYPE_ID_NAME_MAP.read() {
            Ok(guard) => guard.get(self).copied(),
            Err(err) => panic!("poisoned mutex accessing stable type name: {}", err),
        }
    }
    
    pub const fn type_name<T>() -> &'static str where T: ?Sized + 'static {
        std::any::type_name::<T>()
    }

    pub const fn hash<T>() -> u64 where T: ?Sized + 'static {
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

    pub fn raw_id(&self) -> u64 {
        self.0
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

// [CommutativeId]

pub(crate) type CommutativeHashValue = u64;
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct CommutativeId(CommutativeHashValue);
pub(crate) const COMMUTATIVE_ID_INIT: CommutativeId = CommutativeId(COMMUTATIVE_HASH_PRIME);
pub(crate) const COMMUTATIVE_HASH_PRIME: CommutativeHashValue = 0x29233AAB26330D; // 11579208931619597

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

impl From<(FamilyId, ComponentType)> for CommutativeId {
    fn from(value: (FamilyId, ComponentType)) -> Self {
        let family_id = value.0;
        let stable_id = value.1.inner();
        (family_id.0).and(&CommutativeId(stable_id.raw_id()))
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