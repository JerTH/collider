//!
//! 
//! ID types for runtime collections, entities, resources & composite types
//! 
//! 

use std::collections::HashSet;
use std::hash::Hash;
use std::fmt::{Debug, Display, self};
use std::sync::{Arc, RwLock};

use crate::StableTypeId;
use crate::component::ComponentType;

/// Core ID type
#[allow(dead_code)]
#[derive(Copy, Clone)]
pub union IdUnion {
    /// Interprets the id as (id, generation, _, flags)
    pub generational: (u32, u16, u8, u8),

    /// Interprets the id as (id, id) for relational links
    pub relational: (u32, u32),

    /// Interprets the id as a raw u64
    /// 
    /// This forces the union into 8 byte alignment
    pub aligned: u64,

    /// Interprets the id as raw bytes
    pub bytes: [u8; 8],
}

impl IdUnion {

    /// Interpret this as a generational ID
    /// 
    /// SAFETY:
    /// Accessing a unions fields is implicitely unsafe, as it requires
    /// fore-knowledge that the data stored in the union is actually
    /// represented by the field we are accessing it as
    /// 
    /// The user of this function MUST be certain that the data
    /// in this [IdUnion] is already a generational id
    #[inline(always)]
    pub unsafe fn as_generational(&self) -> GenerationalId {
        GenerationalId::from(self)
    }

    /// Interpret this as a relational ID
    /// 
    /// SAFETY: 
    /// The user of this function MUST be certain that the data
    /// in this [IdUnion] is already a relational id
    #[inline(always)]
    pub unsafe fn as_relational(&self) -> RelationalId {
        RelationalId::from(self)
    }

    #[inline(always)]
    pub unsafe fn as_u64(&self) -> u64 {
        self.aligned
    }
}

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

pub struct GenerationalId {
    inner: IdUnion,
}

impl From<&IdUnion> for GenerationalId {
    fn from(value: &IdUnion) -> Self {
        unsafe { debug_assert_eq!(0u8, value.generational.2); }

        GenerationalId {
            inner: *value
        }
    }
}

impl GenerationalId {
    #[inline(always)]
    pub fn id(&self) -> u32 {
        unsafe { self.inner.generational.0 }
    }

    #[inline(always)]
    pub fn generation(&self) -> u16 {
        unsafe { self.inner.generational.1 }
    }

    #[inline(always)]
    pub fn flags(&self) -> u8 {
        unsafe { self.inner.generational.3 }
    }
}

pub struct RelationalId {
    inner: IdUnion,
}

impl From<&IdUnion> for RelationalId {
    fn from(value: &IdUnion) -> Self {
        unsafe { debug_assert_ne!(value.relational.0, value.relational.1); }
        
        RelationalId {
            inner: *value
        }
    }
}

impl RelationalId {
    #[inline(always)]
    pub fn primary(&self) -> u32 {
        unsafe { self.inner.relational.0 }
    }

    #[inline(always)]
    pub fn secondary(&self) -> u32 {
        unsafe { self.inner.relational.1 }
    }
}


/// An `EntityId` uniquely identifies a single entity
/// 
/// Consuming code uses `EntityId` instances to reference entities in the `EntityDatabase`
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct EntityId(IdUnion);

impl EntityId {
    #[inline(always)]
    pub unsafe fn inner(&self) -> IdUnion {
        self.0
    }
    
    /// Returns a new [EntityId] with the same ID as [self]
    /// but with its generation value incremented
    #[inline(always)]
    pub fn next_generation(&self) -> Self {
        unsafe {
            EntityId(IdUnion { generational: (
                self.inner().generational.0,
                // Using wrapping addition here introduces a small
                // risk of collision if one entity id incurs more than
                // 65536 generations, however, this is deemed preferable
                // to panicking in the case of generations overflowing,
                // the user is simply expected to have discarded very old
                // generation id's by the time this overflow happens
                self.inner().generational.1.wrapping_add(1),
                self.inner().generational.2,
                self.inner().generational.3,
            )})
        }
    }
}

impl From<u32> for EntityId {
    fn from(value: u32) -> Self {
        EntityId(IdUnion { generational: (value as u32, 0, 0, 0) })
    }
}

impl Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "E[{:?}]", self.0)
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColumnKey(CommutativeId);

impl ColumnKey {
    fn raw(&self) -> u64 {
        self.0.raw()
    }
}

/// Combines a family ID and a component type id into a column key
/// Each family can have exactly one column of a given type
impl From<(FamilyId, ComponentType)> for ColumnKey {
    fn from(value: (FamilyId, ComponentType)) -> Self {
        let (fc, cc) = (value.0.inner(), CommutativeId::from(value.1.inner()));
        ColumnKey(fc.and(&cc))
    }
}

impl Display for ColumnKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "COL_{:#X}", self.raw())
    }
}






/// A `ComponentId` uniquely identifies a single component within a column of components
/// 
/// These are functionally the same as `EntityId`'s
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct ComponentId(IdUnion);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FamilyId(CommutativeId);

impl FamilyId {
    fn inner(&self) -> CommutativeId {
        self.0
    }
}

impl Display for FamilyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.0)
    }
}

impl<'i> FromIterator<&'i ComponentType> for FamilyId {
    fn from_iter<T: IntoIterator<Item = &'i ComponentType>>(iter: T) -> Self {
        FamilyId(CommutativeId::from_iter(iter.into_iter().map(|id| id.inner().raw())))
    }
}

/// An immutable set of family id's

pub(crate) type FamilyIdSetImpl = HashSet<FamilyId>; // INVARIANT: This type MUST NOT accept duplicates 

#[derive(Debug)]
struct FamilyIdSetInner {
    #[allow(dead_code)] set_id: CommutativeId,
    
    set: FamilyIdSetImpl,
}

#[derive(Debug, Clone)]
pub struct FamilyIdSet {
    ptr: Arc<RwLock<FamilyIdSetInner>>, // thread-local?
}

impl FamilyIdSet {
    pub fn contains(&self, id: &FamilyId) -> bool {
        let a = self.ptr.read().expect("unable to read familyid set");
        a.set.contains(id)
    }

    pub fn insert(&self, family_id: FamilyId) {
        self.ptr.write().expect("unable to write familyid set").set.insert(family_id);
    }

    pub fn clone_into_vec(&self) -> Vec<FamilyId> {
        let mut collection = Vec::new();
        let a = self.ptr.read().expect("unable to read familyid set");
        a.set.iter().for_each(|id| {
            collection.push(id.clone());
        });
        collection
    }
}

impl<'i> FromIterator<FamilyId> for FamilyIdSet {
    fn from_iter<I: IntoIterator<Item = FamilyId>>(iter: I) -> Self {
        let set: FamilyIdSetImpl = iter.into_iter().collect();
        let set_id = CommutativeId::from_iter(set.iter().map(|id| id.0));
        FamilyIdSet { ptr: Arc::new(RwLock::new(FamilyIdSetInner { set_id, set })) }
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


// [CommutativeId]

pub(crate) type CommutativeHashType = u64;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct CommutativeId(CommutativeHashType);
pub(crate) const COMMUTATIVE_ID_INIT: CommutativeId = CommutativeId(COMMUTATIVE_HASH_PRIME);
pub(crate) const COMMUTATIVE_HASH_PRIME: CommutativeHashType = 0x29233AAB26330D; // 11579208931619597

impl CommutativeId {
    #[allow(dead_code)]
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

    pub fn raw(&self) -> u64 {
        self.0
    }
}

impl From<StableTypeId> for CommutativeId {
    fn from(value: StableTypeId) -> Self {
        CommutativeId(value.raw())
    }
}

impl FromIterator<CommutativeId> for CommutativeId {
    fn from_iter<T: IntoIterator<Item = CommutativeId>>(iter: T) -> Self {
        iter.into_iter().fold(COMMUTATIVE_ID_INIT, |acc, x| {
            CommutativeId::combine(&acc, &x)
        })
    }
}

impl FromIterator<CommutativeHashType> for CommutativeId {
    fn from_iter<T: IntoIterator<Item = CommutativeHashType>>(iter: T) -> Self {
        iter.into_iter().fold(COMMUTATIVE_ID_INIT, |acc, x| {
            CommutativeId::combine(&acc, &CommutativeId(x))
        })
    }
}

impl<'i> FromIterator<&'i CommutativeHashType> for CommutativeId {
    fn from_iter<T: IntoIterator<Item = &'i CommutativeHashType>>(iter: T) -> Self {
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
    let id_a = CommutativeId::from_iter(iter_a.into_iter().map(|i| i.raw()));
    let id_aa = CommutativeId::from_iter(iter_aa.into_iter().map(|i| i.raw()));
    let id_b = CommutativeId::from_iter(iter_b.into_iter().map(|i| i.raw()));
    let id_c = CommutativeId::from_iter(iter_c.into_iter().map(|i| i.raw()));
    
    assert_ne!(id_a, id_b);
    assert_ne!(id_a, id_c);
    assert_ne!(id_b, id_c);

    assert_eq!(id_a, id_aa)
}