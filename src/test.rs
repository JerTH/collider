use std::marker::PhantomData;

use collider_core::{ DatabaseSelection, results::TransformationResult, component::ComponentType, id::{FamilyId, FamilyIdSet} };
use collider_proc::{ component, selection };
use crate::{indexes::spatial::{Spatial, SpatialVector, Nearby}, Write, Read, transform::Global, components::ComponentTypeSet};
use crate::EntityDatabase;

#[derive(Debug, Default, Clone)]
struct Vec3(f64, f64, f64);

impl SpatialVector for Vec3 {
    fn as_f64_tuple(&self) -> (f64, f64, f64) {
        (self.0, self.1, self.2)
    }
}

#[component]
struct Physics {
    pos: Vec3,
    vel: Vec3,
    acc: Vec3,
}

#[component]
struct Time {
    tick: u64,
    delta: f64,
    clock: f64,
}

impl Spatial for Physics {
    type Vector = Vec3;
    type Scalar = f64;

    fn position(&self) -> Self::Vector {
        todo!()
    }

    fn size_radius(&self) -> Self::Scalar {
        todo!()
    }
}

#[component]
struct Team {
    team: usize,
}

#[component]
struct Health {
    maximum: usize,
    current: usize,
}

#[component]
struct Explosion {
    radius: f64,
    damage: usize,
    time: f64,
}

/// A Transformation Selection
/// 
/// Tagged with the "selection" attribute
/// 
/// Transformation Selection's are the heart of operating on data in an
/// [EntityDatabase].
/// 
/// Transformations run on an [EntityDatabase] are plain functions
/// that always accept a single argument, a single struct that has been
/// tagged as a "selection".
/// 
/// Selection structs are special in that they never actually hold any
/// data we're interested in
#[selection]
struct ExplosionData {
    // Fetch the "Explosion" component for every entity which has it
    // 
    // By updating the explosion here, we might drive another system
    // which animates the explosion, and another system which cues
    // different audio samples. Where this system has write access, these
    // other systems would only need read access, and so they could run 
    // together in parallel
    explosion: Write<Explosion>,
    
    // Fetch the global "Time" component. Global components are always
    // accessed immutably from regular transformations
    time: Global<Time>,

    // Run a "nearby" query predicated on "physics" components,
    // which fetches the "physics" and "health" components of every
    // entity which it matches that has both. queries are lazy, they
    // dont actually do anything until they are iterated.
    // 
    // Here, [Nearby<C, S>] is a query implemented with the [IndexQuery]
    // trait. It operates on a [SpatialIndex] which in turn is implemented
    // with the [DatabaseIndex] trait. Indices and index queries are a
    // powerful way to augment transformations with specific information
    // that answers specific quesitons about the data in an [EntityDatabase]
    targets: Nearby<Physics, ExplosionTargets<'db>>,
}

/// This selection has the same functionality as the above, and could be
/// used by a transformation in the same way, but in this case, it's actually
/// being used by the "Nearby" query in the "ExplosionData" selection
/// 
/// We need this second selection to tell the query what components it should
/// fetch from the candidate entities it finds.
/// 
/// It's important to note that because this selection is referenced in the
/// "ExplosionData" selection, any read or write access included in this
/// selection will also be included in the "ExplosionData" read and write
/// access set, and the transformations which can run in parallel will be
/// limited thusly
#[selection]
struct ExplosionTargets {
    physics: Read<Physics>,
    health: Write<Health>,
}

//trait Selects<C> {
//    type Ref;
//    fn read<'db>(db: &'db EntityDatabase) -> Option<ComponentIterator<Self::Ref>> { None }
//    fn write<'db>(db: &'db EntityDatabase) -> Option<ComponentIterator<Self::Ref>> { None }
//}
//
//trait Queries<Q: IndexQuery> {}
//
//// generated impls
//impl<'db> Selects<Explosion> for ExplosionData {
//    type Ref = &'db mut Explosion;
//
//    fn read(db: &'db EntityDatabase) -> Option<ComponentIterator<Self::Ref>> { None }
//    fn write(db: &'db EntityDatabase) -> Option<ComponentIterator<Self::Ref>> {
//        Some(ComponentIterator::new())
//    }
//}
//
//impl<'db> Queries<Nearby<Physics, (Read<Physics>, Write<Health>)>> for ExplosionData {
//    
//}

/// A transformation. Transformations always have exactly one argument, a
/// transformation selection struct specially marked with the [selection]
/// attribute these structs are never conventionally constructed, they act
/// as a specialized accessor into an [EntityDatabase]
pub fn explosion(data: ExplosionData) -> TransformationResult {
    //for target in data.targets {
    //    let (nearby_physics, nearby_health) = target;
    //
    //}
    let delta = data.time().delta;
    let targets = data.targets();
    
    for explosion in &data {
        let a = explosion;
    }
    
    Ok(())
}

fn run_transformation<'db, D, F, S>(db: &'db EntityDatabase, f: F)
where
    F: Fn(S) -> TransformationResult,
    S: DatabaseSelection,
{
    let row_components = Vec::from_iter(S::READS.iter().chain(S::WRITES).cloned());
    let component_set = ComponentTypeSet::from(row_components.clone());
    //let matching_families: Vec<FamilyId> = db
    //    .query_mapping::<ComponentTypeSet, FamilyIdSet>(&component_set)
    //    .expect("expected component family")
    //    .clone_to_vec();
}

pub struct RowsIter<R> {
    _marker: PhantomData<R>
}

//#[macro_export]
//macro_rules! impl_rows_iterator {
//    ($([$t:ident, $i:tt]),*) => {
//        #[allow(unused_parens)]
//        impl<$($t),*> Iterator for RowsIter<($($t,)*)> {
//            type Item = ($($t),*);
//            
//            fn next(&mut self) -> Option<Self::Item> {
//                todo!()
//            }
//        }
//    };
//}
//
//
//#[macro_export]
//macro_rules! impl_database_selection {
//    ($([$t:ident, $i:tt]),*) => {
//        #[allow(unused_parens)]
//        impl<$($t),+> crate::test::DatabaseSelection for ($($t,)+)
//        {
//            type Row     = ($($t),+);
//            type RowIter = RowsIter<($($t,)*)>;
//        }
//    }
//}

//impl_database_selection!([A, 0]);
//impl_database_selection!([A, 0], [B, 1]);
//
//impl_rows_iterator!([A, 0]);
//impl_rows_iterator!([A, 0], [B, 1]);
//impl_database_selection!([A, 0], [B, 1], [C, 2]);
