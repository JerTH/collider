//! Indexing
//! 
//! This module is for creating and using indexes attached to an [EntityDatabase]
//! 
//! Indexes (Indices) are used to speed up access into the [EntityDatabase] for more
//! specialized queries

use crate::{Component, EntityId, components::ComponentType, database::reckoning::AnyPtr};

/// An [IndexQuery] represents a question to be asked of a [DbIndex] attached to an [EntityDatabase]
pub trait IndexQuery<'i, C: Component> {
    type Index;

    fn on_index(query: Self, index: &'i Self::Index) -> impl Iterator<Item = EntityId> + 'i
    where
        Self::Index: DbIndex<C>;
}

/// A [DbIndex] describes an index of data stored adjacent to an [EntityDatabase] which
/// organizes its own list of [EntityId]'s it's interest in based on some [Component].
/// 
/// When a [DbIndex] is added to an [EntityDatabase] a hook is created to allow the index
/// to respond to changes to the [Component] type which the [IndexQuery] is interested in.
/// These changes are used by the [DbIndex] to organize, relate, associate, or otherwise
/// keep track of, interesting relationships between entities.
/// 
/// [DbIndex]'s can be queried using an [IndexQuery]. When an [IndexQuery] is executed
/// it is allowed to use whatever data is stored in its associated [DbIndex] to build a
/// list of entities which match whatever predicate the query is meant to satisfy.
/// This list of entities is then used to accelerate a [Transformation] being executed
/// on the [EntityDatabase]
pub trait DbIndex<C: Component>: Default {
    /// Returns an iterator over every [EntityId] currently indexed
    fn indexed<'i>(&'i self) -> impl Iterator<Item = EntityId> + 'i;

    /// Called whenever a component we're interested in changes
    fn on_change<'i>(&self, entity: &EntityId, new_value: &C);

    /// Runs an [IndexQuery] on a [DbIndex], returning an iterator over its results
    fn query<'i, Q: IndexQuery<'i, C> + 'i>(index: &'i Q::Index, query: Q) -> impl Iterator<Item = EntityId> + 'i
    where
        <Q as IndexQuery<'i, C>>::Index: DbIndex<C>,
    {
        Q::on_index(query, &index)
    }
}

#[derive(Debug)]
pub struct Index {
    associated: ComponentType,
    ptr: AnyPtr,
}
