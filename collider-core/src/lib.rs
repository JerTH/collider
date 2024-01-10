//! 
//! 
//! Collider Core Lib
//! 
//! 
#![feature(lazy_cell)]
#![feature(const_type_name)]
#![feature(const_trait_impl)]

pub mod component;
pub mod id;
pub mod any;
pub mod tyid;
pub mod mapping;
pub mod indexing;
pub mod select;
pub mod database;
pub mod results;

mod prelude {
    pub use crate::component::Component;
    pub use crate::id::EntityId;
    pub use crate::tyid::StableTypeId;
    pub use crate::database::EntityDatabase;
    pub use crate::select::DatabaseSelection;
}

pub use prelude::*;