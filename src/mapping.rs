use std::{sync::RwLock, collections::HashMap};
use collider_core::{id::{FamilyId, FamilyIdSet}, component::ComponentType, mapping::{GetDbMap, DbMapping}};
use crate::{components::ComponentTypeSet, EntityId, transfer::TransferGraph};

impl<'db> GetDbMap<'db, (ComponentTypeSet, FamilyId)> for DbMaps {
    fn get(
        &self,
        from: &<(ComponentTypeSet, FamilyId) as DbMapping>::From,
    ) -> Option<<(ComponentTypeSet, FamilyId) as DbMapping>::To> {
        self.component_group_to_family
            .read()
            .ok()
            .and_then(|g| g.get(from).cloned())
    }

    fn mut_map(&'db self) -> Option<<(ComponentTypeSet, FamilyId) as DbMapping>::Guard> {
        self.component_group_to_family.write().ok()
    }
}

impl<'db> GetDbMap<'db, (EntityId, FamilyId)> for DbMaps {
    fn get(
        &self,
        from: &<(EntityId, FamilyId) as DbMapping>::From,
    ) -> Option<<(EntityId, FamilyId) as DbMapping>::To> {
        self.entity_to_owning_family
            .read()
            .ok()
            .and_then(|g| g.get(from).cloned())
    }

    fn mut_map(&'db self) -> Option<<(EntityId, FamilyId) as DbMapping>::Guard> {
        self.entity_to_owning_family.write().ok()
    }
}

impl<'db> GetDbMap<'db, (ComponentType, FamilyIdSet)> for DbMaps {
    fn get(
        &self,
        from: &<(ComponentType, FamilyIdSet) as DbMapping>::From,
    ) -> Option<<(ComponentType, FamilyIdSet) as DbMapping>::To> {
        self.families_containing_component
            .read()
            .ok()
            .and_then(|g| g.get(from).cloned())
    }

    fn mut_map(&'db self) -> Option<<(ComponentType, FamilyIdSet) as DbMapping>::Guard> {
        self.families_containing_component.write().ok()
    }
}

impl<'db> GetDbMap<'db, (ComponentTypeSet, FamilyIdSet)> for DbMaps {
    fn get(
        &self,
        from: &<(ComponentTypeSet, FamilyIdSet) as DbMapping>::From,
    ) -> Option<<(ComponentTypeSet, FamilyIdSet) as DbMapping>::To> {
        self.families_containing_set
            .read()
            .ok()
            .and_then(|g| g.get(from).cloned())
    }

    fn mut_map(&'db self) -> Option<<(ComponentTypeSet, FamilyIdSet) as DbMapping>::Guard> {
        self.families_containing_set.write().ok()
    }
}

impl<'db> GetDbMap<'db, (FamilyId, ComponentTypeSet)> for DbMaps {
    fn get(
        &self,
        from: &<(FamilyId, ComponentTypeSet) as DbMapping>::From,
    ) -> Option<<(FamilyId, ComponentTypeSet) as DbMapping>::To> {
        self.components_of_family
            .read()
            .ok()
            .and_then(|g| g.get(from).cloned())
    }

    fn mut_map(&'db self) -> Option<<(FamilyId, ComponentTypeSet) as DbMapping>::Guard> {
        self.components_of_family.write().ok()
    }
}

impl<'db> GetDbMap<'db, (FamilyId, TransferGraph)> for DbMaps {
    fn get(
        &self,
        from: &<(FamilyId, ComponentTypeSet) as DbMapping>::From,
    ) -> Option<<(FamilyId, TransferGraph) as DbMapping>::To> {
        self.transfer_graph_of_family
            .read()
            .ok()
            .and_then(|g| g.get(from).cloned())
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
#[derive(Debug)]
pub struct DbMaps {
    component_group_to_family: RwLock<HashMap<ComponentTypeSet, FamilyId>>,
    entity_to_owning_family: RwLock<HashMap<EntityId, FamilyId>>,
    families_containing_component: RwLock<HashMap<ComponentType, FamilyIdSet>>,
    families_containing_set: RwLock<HashMap<ComponentTypeSet, FamilyIdSet>>,
    components_of_family: RwLock<HashMap<FamilyId, ComponentTypeSet>>,
    transfer_graph_of_family: RwLock<HashMap<FamilyId, TransferGraph>>,
    // TODO: The traits that back DbMaps are designed to make
    // extensibility possible, dynamic mappings/extensions are
    // a future addition to be explored
}

impl<'db> DbMaps {
    pub(crate) fn new() -> Self {
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
    pub fn get_map<M>(&self, from: &M::From) -> Option<M::To>
    where
        M: DbMapping<'db>,
        M::To: 'db,
        Self: GetDbMap<'db, M>,
    {
        <Self as GetDbMap<'db, M>>::get(&self, from)
    }

    pub fn mut_map<M>(&'db self) -> Option<M::Guard>
    where
        M: DbMapping<'db>,
        M::Map: 'db,
        Self: GetDbMap<'db, M>,
    {
        <Self as GetDbMap<'db, M>>::mut_map(&self)
    }
}
