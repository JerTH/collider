use std::{collections::BTreeSet, sync::Arc, fmt::{Debug, Display}};
use collider_core::component::ComponentType;
use itertools::Itertools;

/// [ComponentTypeSet]
/// 
/// A unique set of components
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentTypeSet {
    ptr: Arc<BTreeSet<ComponentType>>, // thread-local?
    id: u64,
}

impl ComponentTypeSet {
    pub fn contains(&self, component: &ComponentType) -> bool {
        self.ptr.contains(component)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ComponentType> {
        self.ptr.iter()
    }

    pub fn names(&self) -> String {
        let out = self.ptr.iter().fold(String::new(), |out, c| {
            out + &String::from(format!("{}, ", c))
        });
        format!("[{}]", out.trim_end_matches([' ', ',']))
    }

    /// Returns number of [ComponentType]'s in this [ComponentTypeSet]
    pub fn len(&self) -> usize {
        self.ptr.len()
    }

    /// Given a [ComponentTypeSet], compute a set of all subsets of
    /// the unique member components.
    pub fn power_set(&self) -> Vec<ComponentTypeSet> {
        self.ptr
            .iter()
            .cloned()
            .powerset()
            .map(|subset| ComponentTypeSet::from(subset))
            .collect()
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

impl<I> From<I> for ComponentTypeSet
where
    I: IntoIterator<Item = ComponentType>,
{
    fn from(iter: I) -> Self {
        let mut id: u64 = 0;
        
        // sum the unique 64 bit id's for each component type
        // and collect them into a set, use the summed id
        // (which should have a similar likelyhood of collision
        // as any 64 bit hash) and use that as the unique id for
        // the set of components
        let set: BTreeSet<ComponentType> = iter
            .into_iter()
            .map(|component_ty| {
                id = id.wrapping_add(component_ty.inner().raw());
                component_ty
            })
            .collect();
        ComponentTypeSet {
            ptr: Arc::new(set),
            id,
        }
    }
}

/// [ComponentDelta]
/// 
/// Describes a component being added or removed from an entity
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ComponentDelta {
    Add(ComponentType),
    Rem(ComponentType),
}
