use std::{sync::RwLockWriteGuard, collections::HashMap};

pub trait DbMapping<'db> {
    type Guard;
    type From;
    type Map;
    type To;
}

impl<'db, F, T> DbMapping<'db> for (F, T)
where
    F: 'db,
    T: 'db + Clone,
{
    type Guard = RwLockWriteGuard<'db, Self::Map>;
    type From = F;
    type Map = HashMap<F, T>;
    type To = T;
}

pub trait GetDbMap<'db, M: DbMapping<'db>> {
    fn get(&self, from: &M::From) -> Option<M::To>;
    fn mut_map(&'db self) -> Option<M::Guard>;
}


