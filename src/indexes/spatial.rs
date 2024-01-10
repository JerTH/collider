/// Implementation of a simple spatial index for an [EntityDatabase]

use std::{collections::HashMap, ops::Div, fmt::Debug, marker::PhantomData};
use collider_core::{*, indexing::{DatabaseIndex, IndexingTransformation, IndexQuery, IndexingRows, IndexQuerySelection}, results::TransformationResult, select::Selects};
use crate::{EntityId, Read};


#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct SpatialIndexGridVector2D(i32, i32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum SpatialIndexEntry {
    /// Entry is in this grid cell
    InGrid(EntityId),
    
    /// Entry is in a nearby grid cell, but its influence extends
    /// into this grid cell. We store the cell that the entity
    /// actually resides in
    Nearby(EntityId, SpatialIndexGridVector2D),
}

impl SpatialIndexEntry {
    pub fn into_inner_entity(&self) -> EntityId {
        match self {
            SpatialIndexEntry::InGrid(entity) => *entity,
            SpatialIndexEntry::Nearby(entity, _) => *entity,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpatialIndex<C: Component + Spatial> {
    grid_size: f64,
    hash_grid: HashMap<SpatialIndexGridVector2D, Vec<SpatialIndexEntry>>,
    _phantom: PhantomData<C>,
}

impl<C: Component + Spatial> Default for SpatialIndex<C> {
    fn default() -> Self {
        Self::new(1000.0)
    }
}

impl<C: Component + Spatial> SpatialIndex<C> {
    pub fn new(grid_size: f64) -> Self {
        Self { grid_size, hash_grid: HashMap::new(), _phantom: PhantomData }
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

impl<C: Component + Spatial> DatabaseIndex for SpatialIndex<C> where C: Component + Spatial {
    type IndexingTransformation = SpatialIndexingTransformation<C>;
    type Data = <SpatialIndexingTransformation<C> as IndexingTransformation>::Data;

    fn indexed<'i>(&'i self) -> impl Iterator<Item = EntityId> + 'i {
        self.hash_grid.iter().map(|bucket| bucket.1).flatten().filter_map(|entry| match entry {
            SpatialIndexEntry::InGrid(entity) => Some(*entity),
            SpatialIndexEntry::Nearby(_, _) => todo!(),
        })
    }

    //fn on_change(&self, entity: &EntityId, new_value: &C) {
    //    let (_size, position) = (new_value.size_radius(), new_value.position());
    //    let (x, y, z) = position.as_f64_tuple();
    //    let s = self.grid_size;
    //    let grid = self.grid((x, y, z));
    //
    //    if self.hash_grid.get(&grid).is_some_and(|item| item.contains(&SpatialIndexEntry::InGrid(*entity))) {
    //        return;
    //    } else {
    //        todo!()
    //    }
    //}
}

/// Query a [SpatialIndex] for all entities within `max_distance` of `position`
/// and optionally not closer than `min_distance`
#[derive(Debug, Default, Clone)]
pub struct Nearby<C, S>
where
    C: Component + Spatial,
    S: IndexQuerySelection,
{
    pub position: (f64, f64, f64),
    pub max_distance: f64,
    pub min_distance: Option<f64>,
    _marker_q: PhantomData<S>,
    _marker_c: PhantomData<C>,
}

impl<C, S> Selects for Nearby<C, S>
where
    C: Component + Spatial,
    S: IndexQuerySelection,
{
    const READS: &'static [component::ComponentType] = S::READS;
    const WRITES: &'static [component::ComponentType] = S::WRITES;
    const GLOBAL: &'static [component::ComponentType] = S::GLOBAL;
}

impl<C: Component + Spatial, S: IndexQuerySelection> IndexQuery for Nearby<C, S>
where
    S: IntoIterator,
{
    type Index = SpatialIndex<C>;
    type Selects = S;
    
    fn find_matches<'db>(query: Self, index: &'db Self::Index) -> impl Iterator<Item = EntityId> + 'db {
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
                    SpatialIndexEntry::Nearby(_, _) => None,
            });
            return iterator
        } else {
            todo!("support for large cell spanning queries")
        }
    }
}



/// To allow indexes to be maintained, an [IndexingTransformation] must be
/// implemented which updates the [DatabaseIndex] with new data in the
/// [crate::EntityDatabase]
#[derive(Debug, Default, Clone)]
pub struct SpatialIndexingTransformation<C: Component + Spatial> {
    _phantom: PhantomData<C>,
}

impl<C: Component + Spatial> IndexingTransformation for SpatialIndexingTransformation<C> {
    type Data = (EntityId, Read<C>);
    type Index = SpatialIndex<C>;

    fn run<D: EntityDatabase>(_data: IndexingRows<Self::Data, D>, _index: &mut Self::Index) -> TransformationResult {
        //for (entity, spatial) in data {
        //    let (_size, position) = (spatial.size_radius(), spatial.position());
        //    let (x, y, z) = position.as_f64_tuple();
        //    let s = index.grid_size;
        //    let grid = index.grid((x, y, z));
//
        //    // Check if the entity is in the grid, and if it is then does
        //    // the grid cell we calculated already contain it. If it doesn't
        //    // contain it, then we have to move it
        //    if index.hash_grid
        //        .get(&grid)
        //        .is_some_and(|item| !item.contains(&SpatialIndexEntry::InGrid(entity)))
        //    {
        //        todo!()
        //    }
        //}
        Ok(())
    }
}



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
    type Vector: SpatialVector + Debug + Default + Clone;
    type Scalar: FloatType + Debug + Default + Clone;

    fn position(&self) -> Self::Vector;
    fn size_radius(&self) -> Self::Scalar;
}

//impl<T, C> Spatial for T
//where
//    C: Spatial,
//    T: ReadWrite<Component = C>,
//{
//    type Vector = <C as Spatial>::Vector;
//    type Scalar = <C as Spatial>::Scalar;
//
//    fn position(&self) -> Self::Vector {
//        todo!()
//    }
//
//    fn size_radius(&self) -> Self::Scalar {
//        todo!()
//    }
//}

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

