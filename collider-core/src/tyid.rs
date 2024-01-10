//!
//! 
//! ID types for compile time types and collections of compile time types
//! 
//! 

use std::{sync::{LazyLock, RwLock}, collections::HashMap, fmt::Debug};

/// A stable `TypeId` which (should) be common across builds. This isn't a guarantee, especially
/// across different rust versions. Used to generate better errors
/// 
/// StableTypeId's are simply a compile time FNV-1a hash of the type name provided by `std::any::type_name()`
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StableTypeId(pub(crate) u64);

static STABLE_TYPE_ID_NAME_MAP: LazyLock<RwLock<HashMap<StableTypeId, &'static str>>>
    = LazyLock::new(|| RwLock::new(HashMap::new()));

// `StableTypeId`
impl StableTypeId {
    pub fn name(&self) -> Option<&'static str> {
        match STABLE_TYPE_ID_NAME_MAP.read() {
            Ok(guard) => guard.get(self).copied(),
            Err(err) => panic!("poisoned mutex accessing stable type name: {}", err),
        }
    }
    
    pub const fn type_name<T>() -> &'static str where T: ?Sized + 'static {
        std::any::type_name::<T>()
    }

    pub const fn hash<T>() -> u64 where T: ?Sized + 'static {
        let name = Self::type_name::<T>();
        const_fnv1a_hash::fnv1a_hash_str_64(name)
    }

    pub const fn of<T>() -> Self where T: ?Sized + 'static {
        let hash = Self::hash::<T>();
        debug_assert!(hash & 0xFFFF != 0);
        StableTypeId(hash)
    }

    /// Registers name information for this type at runtime for richer debug messages
    /// 
    /// This function associates a `StableTypeId` with an intrinsic type name at runtime, it is necessary to
    /// call `register_debug_info` *before* any calls to `name` in order to get the types name
    pub fn register_debug_info<T>() where T: ?Sized + 'static {
        let name = Self::type_name::<T>();
        let tyid = Self::of::<T>();

        match STABLE_TYPE_ID_NAME_MAP.write() {
            Ok(mut guard) => guard.insert(tyid, name),
            Err(err) => panic!("poisoned mutex registering stable type debug info: {}", err),
        };
    }
    
    pub fn raw(&self) -> u64 {
        self.0
    }
}

impl Debug for StableTypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match Self::name(&self) {
            Some(name) => write!(f, "[{}]", name),
            None => write!(f, "[unknown type]"),
        }
    }
}
