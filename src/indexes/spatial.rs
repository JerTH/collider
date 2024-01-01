/// Implementation of a simple spatial index for an [EntityDatabase]

use std::{collections::HashMap, ops::Div, fmt::Debug, marker::PhantomData};
use crate::{EntityId, Component, indexing::{DatabaseIndex, IndexQuery, IndexingTransformation, IndexingRows}, Read, transform::{Rows, TransformationResult}};



#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct SpatialIndexGridVector2D(i32, i32);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum SpatialIndexEntry {
    /// Entry is in this grid cell
    InGrid(EntityId),
    
    /// Entry is in a nearby grid cell, but its influence extends
    /// into this grid cell
    Nearby(EntityId),
}

impl SpatialIndexEntry {
    fn into_inner(&self) -> EntityId {
        match self {
            SpatialIndexEntry::InGrid(entity) => *entity,
            SpatialIndexEntry::Nearby(entity) => *entity,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpatialIndex {
    grid_size: f64,
    hash_grid: HashMap<SpatialIndexGridVector2D, Vec<SpatialIndexEntry>>,
}

impl Default for SpatialIndex {
    fn default() -> Self {
        Self::new(1000.0)
    }
}

impl SpatialIndex {
    fn new(grid_size: f64) -> Self {
        Self { grid_size, hash_grid: HashMap::new(), }
    }

    fn grid(&self, position: (f64, f64, f64)) -> SpatialIndexGridVector2D {
        let s = self.grid_size;
        let (x, y, z) = (position.0, position.1, position.2);
        SpatialIndexGridVector2D(x.div(s).floor() as i32, y.div(s).floor() as i32)
    }

    fn neighboring_xy(&self, grid: SpatialIndexGridVector2D) -> [SpatialIndexGridVector2D; 9] {
        let (x, y) = (grid.0, grid.1);
        [
            // center
            SpatialIndexGridVector2D(x, y),

            // sides
            SpatialIndexGridVector2D(x-1, y),
            SpatialIndexGridVector2D(x+1, y),
            SpatialIndexGridVector2D(x, y-1),
            SpatialIndexGridVector2D(x, y+1),

            // corners
            SpatialIndexGridVector2D(x-1, y+1),
            SpatialIndexGridVector2D(x+1, y+1),
            SpatialIndexGridVector2D(x-1, y-1),
            SpatialIndexGridVector2D(x+1, y-1),
        ]
    }
}

impl<C> DatabaseIndex<C> for SpatialIndex where C: Component + Spatial {
    fn indexed<'i>(&'i self) -> impl Iterator<Item = EntityId> + 'i {
        self.hash_grid.iter().map(|bucket| bucket.1).flatten().filter_map(|entry| match entry {
            SpatialIndexEntry::InGrid(entity) => Some(*entity),
            SpatialIndexEntry::Nearby(_) => todo!(),
        })
    }

    fn on_change(&self, entity: &EntityId, new_value: &C) {
        let (_size, position) = (new_value.size_radius(), new_value.position());
        let (x, y, z) = position.as_f64_tuple();
        let s = self.grid_size;
        let grid = self.grid((x, y, z));

        if self.hash_grid.get(&grid).is_some_and(|item| item.contains(&SpatialIndexEntry::InGrid(*entity))) {
            return;
        } else {
            todo!()
        }
    }
}

/// To allow indexes to be maintained, an [IndexingTransformation] must be
/// implemented that performs the update. 
struct SpatialIndexingTransformation<C: Component + Spatial> {
    _phantom: PhantomData<C>,
}


impl<C: Component + Spatial> IndexingTransformation for SpatialIndexingTransformation<C> {
    type Data = (EntityId, Read<C>);
    type Index = SpatialIndex;

    fn run(data: IndexingRows<Self::Data>, index: &mut Self::Index) -> TransformationResult {
        for (entity, spatial) in data {
            index.on_change(&entity, spatial)
        }
        Ok(())
    }
}


/// Query a [SpatialIndex] for all entities within `max_distance` of `position`
/// and optionally not closer than `min_distance`
#[derive(Debug, Default, Clone)]
pub struct Nearby<C> where C: Component + Spatial {
    pub position: <C as Spatial>::V,
    pub max_distance: <C as Spatial>::S,
    pub min_distance: Option<<C as Spatial>::S>
}



impl<C: Component, T: Spatial + Component> IndexQuery<C> for Nearby<T>
where
    C: Component + Spatial
{
    type Index = SpatialIndex;
    type Component = C;
    
    fn on_index<'db>(query: Self, index: &'db Self::Index) -> impl Iterator<Item = EntityId> + 'db {
        let (query_location, query_radius) = (query.position.as_f64_tuple(), query.max_distance.as_f64());

        let grid = index.grid(query_location);
        if query_radius < (index.grid_size * 0.5) {
            let neighboring = index.neighboring_xy(grid);
            let iterator = neighboring.into_iter()
                .map(|cell_vector| index.hash_grid.get(&cell_vector))
                .filter_map(|option_cell| option_cell)
                .flat_map(|cell| cell.iter())
                .filter_map(|entry| match entry {
                    SpatialIndexEntry::InGrid(entity) => Some(*entity),
                    SpatialIndexEntry::Nearby(_) => None,
            });
            return iterator
        } else {
            todo!("support for large cell spanning queries")
        }
    }
}




/// In order to allow [IndexQuery]'s to use the normal [Transformation]
/// iterator syntax, they must implement [Component]. 
impl<'i, C: Component + Spatial> Component for Nearby<C> {}





// Some types to make [SpatialIndex] generic over a range of
// user defined components. These aren't necessary for typical
// index implementations, as you can set them up to work on
// just a single type to simplify things


/// Describes any object with a spatial attribute
/// 
/// V: Vector Type, typically a 2 or 3 vector, but can be anything that has a defined
///     position in a space and a defined distance to other positions
/// S: Scalar Type, any real number type. Used to describe "size" when performing queries,
///     if the data doesn't have a defined size then () can be substituted
pub trait Spatial {
    type V: SpatialVector + Debug + Default + Clone;
    type S: FloatType + Debug + Default + Clone;

    fn position(&self) -> Self::V;
    fn size_radius(&self) -> Self::S;
}

/// Float type generic over f32 and f64
pub trait FloatType {
    fn as_f64(&self) -> f64;
}

impl FloatType for f64 {
    #[inline(always)]
    fn as_f64(&self) -> f64 {
        *self
    }
}

impl FloatType for f32 {
    #[inline(always)]
    fn as_f64(&self) -> f64 {
        *self as f64
    }
}

/// 3-Vector type generic over f32 and f64
pub trait SpatialVector {
    fn as_f64_tuple(&self) -> (f64, f64, f64);
}

impl<S> SpatialVector for (S, S, S) where S: FloatType {
    fn as_f64_tuple(&self) -> (f64, f64, f64) {
        (self.0.as_f64(), self.1.as_f64(), self.2.as_f64())
    }
}

