use std::any::Any;
use std::collections::BTreeSet;
use std::fmt::Debug;
use std::ops::Deref;
use std::sync::Arc;

use crate::id::StableTypeId;

pub trait Component: Any + Debug + 'static {}

/// A `ComponentType` uniquely identifies a single kind of component. Regular components
/// are synonymous with a single Type, but relational components are unique to a run-time instance
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentType(StableTypeId);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) enum ComponentDelta {
    Add(ComponentType),
    Remove(ComponentType),
}

/// A set of unique component types
#[derive(Clone, Default, PartialEq, Eq, Hash, Debug)]
pub(crate) struct ComponentTypeSet(pub(crate) Arc<BTreeSet<ComponentType>>);


/// Tracks all components that have been seen
#[derive(Default, Clone, Debug)]
pub(crate) struct ComponentRegistry {
    seen: Vec<ComponentType>,
}

// Impl's

// `Component` trait
impl Component for () {}

// `ComponentType`
impl ComponentType {
    pub fn of<T>() -> Self where T: Component {
        StableTypeId::of::<T>().into()
    }
}

impl Debug for ComponentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ComponentType({:X?}:{:?})", self.0.0, self.0)
    }
}

impl From<StableTypeId> for ComponentType {
    fn from(value: StableTypeId) -> Self {
        ComponentType(value)
    }
}

// `ComponentTypeSet`
impl ComponentTypeSet {
    fn empty() -> Self {
        ComponentTypeSet(Arc::new(BTreeSet::new()))
    }
}

impl FromIterator<ComponentType> for ComponentTypeSet {
    fn from_iter<T: IntoIterator<Item = ComponentType>>(iter: T) -> Self {
        ComponentTypeSet(Arc::new(BTreeSet::from_iter(iter)))
    }
}

impl From<ComponentType> for ComponentTypeSet {
    fn from(value: ComponentType) -> Self {
        ComponentTypeSet(Arc::new([value].iter().cloned().collect()))
    }
}

impl Deref for ComponentTypeSet {
    type Target = BTreeSet<ComponentType>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ComponentRegistry {
    pub fn new() -> Self {
        ComponentRegistry {
            seen: Vec::with_capacity(2^8usize) // reasonable alloc size but can grow if needed
        }
    }

    /// Checks if the given component type is known by this registery
    /// Returns `true` if it is, `false` if it is not
    pub fn seen<T: Component>(&self) -> bool {
        let val = ComponentType::of::<T>();
        self.seen_val(&val)
    }

    pub fn seen_val(&self, val: &ComponentType) -> bool {
        self.seen.binary_search(&val).is_ok()
    }

    /// Registers the component
    pub fn register<T: Component>(&mut self) {
        let val = ComponentType::of::<T>();
        StableTypeId::register_debug_info::<T>();
        self.seen.push(val);
        self.seen.sort();
    }
}
