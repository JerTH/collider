//!
//! 
//! Component support and functionality
//! 
//! 

use std::fmt::{Debug, Display};

use crate::StableTypeId;

/// [Component]
/// 
/// The core component trait
/// 
/// Users must implement this trait on any struct or enum they wish to
/// use as components in an [crate::EntityDatabase]
pub trait Component: Default + Debug + Clone + 'static {
    fn is_query_component() -> bool { false }
}

/// Component implementation for the unit type
/// Every entity automatically gets this component when it's created
impl Component for () {}


/// [ComponentType]
/// 
/// A unique identifier for a component
#[derive(Copy, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentType(StableTypeId);

impl ComponentType {
    pub const fn of<C: Component>() -> Self {
        Self(StableTypeId::of::<C>())
    }

    pub fn inner(&self) -> StableTypeId {
        return self.0;
    }
}

impl From<StableTypeId> for ComponentType {
    fn from(value: StableTypeId) -> Self {
        Self(value)
    }
}

impl Display for ComponentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let full_name = self.0.name().unwrap_or("{unknown}");
        let split_str = full_name.rsplit_once("::");
        let substring = split_str.unwrap_or((full_name, full_name)).1;
        write!(f, "{}", substring)
    }
}
