use std::error::Error;
use std::fmt::Debug;
use std::hash::Hash;

use crate::mapping::GetDbMap;

pub trait EntityDatabase: Debug {
    fn query_mapping<'db, K, V, M> (
        &'db self,
        key: &'db K
    ) -> Option<V>
    where
        K: Eq + Hash,
        V: Clone + 'db,;
        //Self::DatabaseMap: GetDbMap<'db, (K, V)>,;
    


    fn update_mapping<'db, K, V, M> (    
        &'db self,
        key: &'db K,
        value: &'db V,
    ) -> Result<(), impl Error>
    where
        K: Debug + Clone + Eq + Hash + 'db,
        V: Debug + Clone + 'db,;
        //Self::DatabaseMap: GetDbMap<'db, (K, V)>,;
    


    fn delete_mapping<'db, K, V>(
        &'db self,
        key: &'db K,
    ) -> Result<(), impl Error>
    where
        K: Debug + Clone + Eq + Hash + 'db,
        V: Debug + Clone + 'db,;
        //Self::DatabaseMap: GetDbMap<'db, (K, V)>,;
}
