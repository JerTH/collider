use std::{collections::BTreeSet, sync::Arc, fmt::{Debug, Display}};
use collider_core::component::ComponentType;
use itertools::Itertools;

/// [ComponentDelta]
/// 
/// Describes a component being added or removed from an entity
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ComponentDelta {
    Add(ComponentType),
    Rem(ComponentType),
}
