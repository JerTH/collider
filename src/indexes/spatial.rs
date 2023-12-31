use std::{collections::HashMap, ops::Div, cell, ptr::NonNull};
use itertools::Itertools;

use crate::{EntityId, Component, indexing::{DbIndex, IndexQuery}};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Debug, Default, Clone)]
pub struct SpatialIndex {
    grid_size: f64,
    hash_grid: HashMap<SpatialIndexGridVector2D, Vec<SpatialIndexEntry>>,
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

pub trait RealNumber {
    fn as_f64(&self) -> f64;
}

impl RealNumber for f64 {
    #[inline(always)]
    fn as_f64(&self) -> f64 {
        *self
    }
}

impl RealNumber for f32 {
    #[inline(always)]
    fn as_f64(&self) -> f64 {
        *self as f64
    }
}

pub trait SpatialVector {
    fn as_f64_tuple(&self) -> (f64, f64, f64);
}

impl<S> SpatialVector for (S, S, S) where S: RealNumber {
    fn as_f64_tuple(&self) -> (f64, f64, f64) {
        (self.0.as_f64(), self.1.as_f64(), self.2.as_f64())
    }
}

/// Describes any object with a spatial attribute
/// 
/// V: Vector Type, typically a 2 or 3 vector, but can be anything that has a defined
///     position in a space and a defined distance to other positions
/// S: Scalar Type, any real number type. Used to describe "size" when performing queries,
///     if the data doesn't have a defined size then () can be substituted
pub trait Spatial {
    type V: SpatialVector;
    type S: RealNumber;

    fn position(&self) -> Self::V;
    fn size_radius(&self) -> Self::S;
}

impl<'index, C> DbIndex<'index, C> for SpatialIndex where C: Component + Spatial {
    fn indexed(&'index self) -> impl Iterator<Item = EntityId> + 'index {
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

pub struct Nearby<T> where T: Spatial {
    position: <T as Spatial>::V,
    radius: <T as Spatial>::S,
}

pub struct NearbyIterator<'i> {
    db_index: &'i SpatialIndex,
    check_cells: [SpatialIndexGridVector2D; 9],
    check_index: usize,
    bucket_index: usize,
}

impl<'i> NearbyIterator<'i> {
    fn new(index: &'i SpatialIndex, cells: [SpatialIndexGridVector2D; 9]) -> Self {
        Self {
            db_index: index,
            check_cells: cells,
            check_index: 0,
            bucket_index: 0,
        }
    }

    fn next_cell(&mut self) {
        self.bucket_index = 0;
        self.check_index += 1;
    }

    fn next_item(&mut self, cell: &SpatialIndexGridVector2D) -> Option<EntityId> {
        match self.db_index.hash_grid.get(cell) {
            Some(bucket) => {
                match bucket.get(self.bucket_index) {
                    Some(entry) => {
                        return Some(entry.into_inner())
                    },
                    None => {
                        self.bucket_index += 1;
                        None
                    },
                }
            },
            None => {
                self.next_cell();
                None
            },
        }
    }
}

impl<'i> Iterator for NearbyIterator<'i> {
    type Item = EntityId;

    fn next(&mut self) -> Option<Self::Item> {
        let mut yields: Option<Self::Item> = None;

        while yields.is_none() {
            let cell = *self.check_cells.get(self.check_index)?;
            yields = self.next_item(&cell);
        }
        return yields
    }
}

impl<'i, C: Component, T: Spatial> IndexQuery<'i, C> for Nearby<T>
where
    C: Component + Spatial
{
    type Index = SpatialIndex;

    fn on_index(query: Self, index: &'i Self::Index) -> impl Iterator<Item = EntityId> + 'i {
        let (query_location, query_radius) = (query.position.as_f64_tuple(), query.radius.as_f64());
        let grid = index.grid(query_location);

        if query_radius < (index.grid_size * 0.5) {
            let neighboring = index.neighboring_xy(grid);
            NearbyIterator::new(index, neighboring)
        } else {
            todo!("support for large cell spanning queries")
        }
    }
}
