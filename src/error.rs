use std::{error::Error, fmt::Display};

use collider_core::{id::FamilyId, component::ComponentTypeSet};

use crate::EntityId;

#[derive(Debug)]
pub enum DbError {
    EntityDoesntExist(EntityId),
    FailedToResolveTransfer,
    FailedToFindEntityFamily(EntityId),
    FailedToFindFamilyForSet(ComponentTypeSet),
    EntityBelongsToUnknownFamily,
    FailedToAcquireMapping,
    ColumnTypeDiscrepancy,
    ColumnAccessOutOfBounds,
    TableDoesntExistForFamily(FamilyId),
    ColumnDoesntExistInTable,
    EntityNotInTable(EntityId, FamilyId),
    UnableToAcquireTablesLock(String),
    FamilyDoesntExist(FamilyId),
    UnableToAcquireLock,
    MoveWithSameColumn,
}

impl Error for DbError {}

impl Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::EntityDoesntExist(entity) => {
                write!(f, "entity {:?} doesn't exist", entity)
            }
            DbError::FailedToResolveTransfer => {
                write!(f, "failed to transfer entity between families")
            }
            DbError::FailedToFindEntityFamily(entity) => {
                write!(f, "failed to find a family for entity {:?}", entity)
            }
            DbError::FailedToFindFamilyForSet(set) => {
                write!(
                    f,
                    "failed to find a family for the set of components {}",
                    set
                )
            }
            DbError::EntityBelongsToUnknownFamily => {
                write!(f, "requested family data is unknown or invalid")
            }
            DbError::FailedToAcquireMapping => {
                write!(f, "failed to acquire requested mapping")
            }
            DbError::ColumnTypeDiscrepancy => {
                write!(f, "column type mismatch")
            }
            DbError::ColumnAccessOutOfBounds => {
                write!(f, "attempted to index a column out of bounds")
            }
            DbError::TableDoesntExistForFamily(family) => {
                write!(f, "table doesn't exist for the given family id {}", family)
            }
            DbError::ColumnDoesntExistInTable => {
                write!(f, "column doesn't exist in the given table")
            }
            DbError::EntityNotInTable(entity, family) => {
                write!(f, "{:?} does not exist in {:?} data table", entity, family)
            }
            DbError::UnableToAcquireTablesLock(reason) => {
                write!(f, "unable to acquire master table lock: {}", reason)
            }
            DbError::FamilyDoesntExist(family) => {
                write!(f, "family doesn't exist: {}", family)
            }
            DbError::UnableToAcquireLock => {
                write!(f, "failed to acquire poisoned lock")
            }
            DbError::MoveWithSameColumn => {
                write!(
                    f,
                    "attempted to move a component to the column it was already in"
                )
            }
        }
    }
}
