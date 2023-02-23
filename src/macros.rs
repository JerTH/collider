
// Macros to make operations interfacing with the `EntityDatabase` more ergonomic
macro_rules! component {
    ($a:ty) => {
        crate::comps::ComponentType::from(crate::id::StableTypeId::of::<$a>())
    };
}

macro_rules! component_type_set {
    ($($ct:ty),+) => {
        {
            let mut __set = std::collections::BTreeSet::new();
            $(
                __set.insert(component!($ct));
            )*
            crate::comps::ComponentTypeSet(Arc::new(__set))
        }
    };
}

macro_rules! family_id_set {
    ($($fi:expr),+) => {
        {
            let mut __set = std::collections::BTreeSet::<FamilyId>::new();
            $(
                __set.insert($fi);
            )*
            crate::family::FamilyIdSet(__set)
        }
    };
}
