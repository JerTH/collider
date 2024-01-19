use std::error::Error;
use std::fmt::Debug;
use std::hash::Hash;

pub trait EntityDatabase: Debug {
    fn get_table<'db, R>(&'db self, family: &crate::id::FamilyId) -> Option<R>;

    fn query_mapping<'db, K, V> (
        &'db self,
        key: &'db K
    ) -> Option<V>
    where
        K: Eq + Hash,
        V: Clone + 'db,;
    
    fn update_mapping<'db, K, V> (    
        &'db self,
        key: &'db K,
        value: &'db V,
    ) -> Result<(), impl Error>
    where
        K: Debug + Clone + Eq + Hash + 'db,
        V: Debug + Clone + 'db,;

    fn delete_mapping<'db, K, V>(
        &'db self,
        key: &'db K,
    ) -> Result<(), impl Error>
    where
        K: Debug + Clone + Eq + Hash + 'db,
        V: Debug + Clone + 'db,;
}
