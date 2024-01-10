//! 
//! Collider is a relational entity database and jobs system which supports fast cacheable queries and quick parallel iteration
//! 

#![feature(const_trait_impl)]
//#![feature(associated_type_defaults)]

// Module declarations
#[macro_use] pub mod indexing_macros;
#[macro_use] pub mod transformation_macros;

pub mod error;
pub mod id;
pub mod borrowed;
pub mod components;
pub mod entity;
pub mod column;
pub mod table;
pub mod transform;
pub mod conflict;
pub mod transfer;
pub mod database;
pub mod mapping;
pub mod indexes;

// Prelude
pub use collider_core::EntityId;
pub use collider_core::Component;
pub use database::EntityDatabase;
pub use transform::{ Phase, Transformation, Read, Write };

mod test;
