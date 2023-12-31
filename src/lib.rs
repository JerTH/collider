//! 
//! Collider is a relational entity database and jobs system which supports fast cacheable queries and quick parallel iteration
//! 

#![feature(const_type_name)]
#![feature(const_trait_impl)]
#![feature(lazy_cell)]
//#![feature(associated_type_defaults)]

// Incomplete feature - monitor with caution
#![feature(return_position_impl_trait_in_trait)]

// Module declarations
#[macro_use]
pub(crate) mod macros;
pub mod id;
pub mod borrowed;
pub mod components;
pub mod column;
pub mod table;
pub mod transform;
pub mod conflict;
pub mod transfer;
pub mod database;
pub mod mapping;
pub mod indexing;
pub mod indexes;

// Use declarations
pub use id::EntityId;
pub use components::Component;
pub use database::EntityDatabase;
pub use transform::{ Phase, Transformation, Read, Write };