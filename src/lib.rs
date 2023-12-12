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

//pub(crate) mod db;
//pub(crate) mod comps;
//pub(crate) mod family;
//pub(crate) mod xform;

pub(crate) mod database; // replaces db

//pub use db::EntityDatabase;
pub use id::EntityId;
//pub use comps::Component;
//pub use xform::Read;
//pub use xform::Write;
//pub use xform::Phase;
//pub use xform::Transformation;
//pub use xform::TransformError;
//pub use xform::TransformSuccess;
//pub use xform::TransformResult;
