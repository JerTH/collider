//! 
//! Collider is a relational entity database and jobs system which supports fast cacheable queries and quick parallel iteration
//! 

#![feature(const_type_name)]
#![feature(const_trait_impl)]
#![feature(lazy_cell)]
//#![feature(associated_type_defaults)]

// Incomplete feature - monitor with caution
#![feature(return_position_impl_trait_in_trait)]

#[macro_use]
pub(crate) mod macros;
pub mod id;
pub mod borrowed;
pub mod column;
pub mod table;
pub mod transform;
pub mod conflict;
pub mod database;

pub use id::EntityId;
