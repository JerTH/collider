use std::collections::BTreeSet;
use std::collections::HashSet;
use std::ops::AddAssign;
use std::ops::Deref;
use std::collections::HashMap;
use std::ops::DerefMut;
use std::sync::Arc;

use crate::comps::ComponentType;
use crate::comps::ComponentTypeSet;
use crate::id::EntityId;
use crate::id::FamilyId;

/// A `Family` is a collection of entities with the same components, when components are
/// added or removed from an entity, the entity is moved between families
#[derive(Clone, Debug)]
pub(crate) struct Family {
    /// A reference to the unique set of component id's which comprise this family
    components: ComponentTypeSet,

    /// When an entity gains or loses a component, it must be moved to a different family.
    /// Here we lazily construct a graph of transfers to make subsequent transfers faster
    transfer_graph: HashMap<ComponentType, FamilyGraphEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FamilyTransform {
    Transfer {
        from: FamilyId,
        dest: FamilyId,
    },
    InitTransfer {
        from: FamilyId,
        set: ComponentTypeSet,
    },
    InitNew(ComponentTypeSet),
    NewEntity(FamilyId),
    NoChange(FamilyId),
}

/// Maps a set of component types to the single associated family
#[derive(Clone, Default, Debug)]
pub(crate) struct ComponentSetFamilyMap(HashMap<ComponentTypeSet, FamilyId>);

/// A set of unique family id's
#[derive(Clone, Default, Debug)]
pub(crate) struct FamilyIdSet(pub(crate) BTreeSet<FamilyId>);

/// Maps a single `ComponentType` to the set of families which contain it by their `FamilyId`'s
#[derive(Clone, Default, Debug)]
pub(crate) struct ContainingFamiliesMap(HashMap<ComponentType, FamilyIdSet>);

/// Maps a single `EntityId` to its corresponding `FamilyId`
#[derive(Clone, Default, Debug)]
pub(crate) struct EntityFamilyMap(HashMap<EntityId, FamilyId>);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) enum FamilyDelta {
    Add(FamilyId),
    Remove(FamilyId),
}

/// A `FamilyGraphEdge` describes how to transfer an entity between families given a new or removed component
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct FamilyGraphEdge {
    pub(crate) component: ComponentType,
    pub(crate) delta: FamilyDelta,
}

#[derive(Default, Clone, PartialEq, Eq, Debug)]
pub struct SubFamilies(Arc<Vec<FamilyId>>);

pub struct SubFamiliesIter {
    data: SubFamilies,
    next: usize,
}

/// Maps component type sets to lists of families where
/// the family contains at least the components in the type set
#[derive(Default, Clone, PartialEq, Eq, Debug)]
pub(crate) struct SubFamilyMap(HashMap<ComponentTypeSet, SubFamilies>);

// Impl's

impl Family {
    pub fn components(&self) -> ComponentTypeSet {
        self.components.clone()
    }

    pub fn try_get_transfer_edge(&self, component: &ComponentType) -> Option<FamilyGraphEdge> {
        self.transfer_graph.get(component).cloned()
    }
}

impl From<ComponentTypeSet> for Family {
    fn from(value: ComponentTypeSet) -> Self {
        Family {
            components: value,
            transfer_graph: HashMap::new(),
        }
    }
}

// `ComponentSetFamilyMap`
impl Deref for ComponentSetFamilyMap {
    type Target = HashMap<ComponentTypeSet, FamilyId>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ComponentSetFamilyMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// `FamilyIdSet`
impl Deref for FamilyIdSet {
    type Target = BTreeSet<FamilyId>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for FamilyIdSet {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// `ContainingFamiliesMap`
impl Deref for ContainingFamiliesMap {
    type Target = HashMap<ComponentType, FamilyIdSet>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ContainingFamiliesMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// `EntityFamilyMap`
impl Deref for EntityFamilyMap {
    type Target = HashMap<EntityId, FamilyId>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for EntityFamilyMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// `SubFamilies`
impl IntoIterator for SubFamilies {
    type Item = FamilyId;

    type IntoIter = SubFamiliesIter;

    fn into_iter(self) -> Self::IntoIter {
        SubFamiliesIter {
            data: self.clone(),
            next: 0,
        }
    }
}

impl Deref for SubFamilies {
    type Target = Vec<FamilyId>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

//impl SubFamilies {
//    fn iter(&self) -> std::collections::hash_set::Iter<FamilyId> {
//        self.0.as_ref().into_iter()
//    }
//}

impl From<HashSet<FamilyId>> for SubFamilies {
    fn from(value: HashSet<FamilyId>) -> Self {
        SubFamilies(Arc::new(value.into_iter().collect()))
    }
}

// `SubFamilyMap`
impl Deref for SubFamilyMap {
    type Target = HashMap<ComponentTypeSet, SubFamilies>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SubFamilyMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// `SubFamiliesIter`
impl Iterator for SubFamiliesIter {
    type Item = FamilyId;

    fn next(&mut self) -> Option<Self::Item> {
        self.next.add_assign(1);
        self.data.get(self.next - 1).cloned()
    }
}
