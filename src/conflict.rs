

use std::{
    cell::{Cell, RefCell},
    collections::{hash_map::IntoValues, HashMap, HashSet},
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

pub trait Dependent {
    type Dependency: PartialEq + Eq + std::hash::Hash;

    // The 'iter lifetime associated with these two functions deserves to
    // be reviewed. The only reason this trait is structured this way is
    // to avoid polluting the code with additional lifetimes. The resulting
    // compromise is that [Self::Dependency]'s yielded by the returned
    // iterators must be copied or cloned, they cannot yield references
    fn dependencies<'iter>(&'iter self) -> impl Iterator<Item = Self::Dependency> + 'iter;
    fn exclusive_dependencies<'iter>(
        &'iter self,
    ) -> impl Iterator<Item = Self::Dependency> + 'iter {
        std::iter::empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConflictColor(usize);
impl ConflictColor {
    fn as_usize(&self) -> usize {
        self.0
    }
}

#[derive(Debug)]
pub struct ConflictGraphNode<K, V, D> {
    key: K,
    val: V,
    dep: HashSet<D>,
    exc: HashSet<D>,
    edges: RefCell<HashSet<usize>>,
    color: Cell<Option<ConflictColor>>,
}

#[derive(Debug)]
pub struct Init<K, V, D>(Vec<ConflictGraphNode<K, V, D>>);

impl<K, V, D> Init<K, V, D> {
    fn new() -> Self {
        Self(Vec::new())
    }
}

impl<K, V, D> Deref for Init<K, V, D> {
    type Target = Vec<ConflictGraphNode<K, V, D>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<K, V, D> DerefMut for Init<K, V, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

type Buckets<K, V> = HashMap<usize, Vec<(K, V)>>;

#[derive(Debug)]
pub struct Built<K, V> {
    buckets: Buckets<K, V>,
}

/// A [ConflictGraph] is an expensive structure to build, and once it's built
/// it is considered entirely immutable, in addition, the data needed to
/// build the graph and the data needed to query a built graph is different.
/// Because of these properties, we use type-state to encode an initializing
/// state and a built state directly into the type system, with different
/// exposed methods and different internal representations on the two
pub trait ConflictGraphState {}
impl<K, V, D> ConflictGraphState for Init<K, V, D> {}
impl<K, V> ConflictGraphState for Built<K, V> {}

const COLOR_ZERO: ConflictColor = ConflictColor(0);

#[derive(Debug)]
pub struct ConflictGraph<
    K,
    V: Dependent,
    State: ConflictGraphState = Init<K, V, <V as Dependent>::Dependency>,
> {
    state: State,
    _k: PhantomData<K>,
    _v: PhantomData<V>,
}

impl<'g, K, V> ConflictGraph<K, V, Init<K, V, <V as Dependent>::Dependency>>
where
    V: Dependent,
    K: Clone + PartialEq + Eq,
    K: std::hash::Hash,
{
    pub fn new() -> Self {
        Self {
            state: Init::new(),
            _k: PhantomData::default(),
            _v: PhantomData::default(),
        }
    }

    pub fn insert(&mut self, key: K, val: V) {
        let dep: HashSet<V::Dependency> = val.dependencies().collect();

        let exc: HashSet<V::Dependency> = val.exclusive_dependencies().collect();

        self.state.push(ConflictGraphNode {
            key,
            val,
            dep,
            exc,
            edges: RefCell::new(HashSet::new()),
            color: Cell::new(None),
        });

        self.resolve_edges()
    }

    /// Create edges where nodes have exclusive dependencies which conflict
    /// with other nodes dependencies or exclusive dependencies
    fn resolve_edges(&mut self) {
        for (first_index, first) in self.state.iter().enumerate() {
            for (other_index, other) in self.state.iter().enumerate() {
                // If we are pointing at ourself, go to the next iteration
                if std::ptr::eq(first, other) {
                    continue;
                }

                for exc in first.exc.iter() {
                    // Chained iterator iterates all of the dependencies and
                    // exclusive dependencies of our other node, we compare
                    // each iterated item with the current exclusive
                    // dependency of the current iterated node
                    for dep in other.dep.iter().chain(other.exc.iter()) {
                        if exc == dep {
                            // We are using a hashset to store edges, thus
                            // duplicates are handled implicitly and we ignore
                            first.edges.borrow_mut().insert(other_index);
                            other.edges.borrow_mut().insert(first_index);
                        }
                    }
                }
            }
        }
    }

    fn color(&mut self) {
        let nodes = &self.state;
        let mut uncolored = nodes.len();
        let mut palette = Vec::from([COLOR_ZERO]);

        while let Some(node) = self.pick_next() {
            let mut available_colors = palette.clone();

            for neighbor_color in node
                .edges
                .borrow()
                .iter()
                .filter_map(|i| nodes.get(*i).and_then(|n| n.color.get()))
            {
                if let Some(pos) = available_colors.iter().position(|c| *c == neighbor_color) {
                    available_colors.remove(pos);
                }
            }

            if let Some(color) = available_colors.first() {
                node.color.set(Some(*color));
            } else {
                palette.push(ConflictColor(palette.len()));
                node.color
                    .set(Some(*palette.last().expect("expected color")));
            }

            uncolored -= 1;
        }
        debug_assert!(uncolored == 0);
    }

    fn pick_next(&self) -> Option<&ConflictGraphNode<K, V, <V as Dependent>::Dependency>> {
        let nodes = &self.state;
        let mut candidate: Option<&ConflictGraphNode<K, V, <V as Dependent>::Dependency>> = None;
        let (mut candidate_colored, mut candidate_uncolored) = (0, 0);

        for node in nodes.iter() {
            // skip already colored nodes
            if node.color.get().is_some() {
                continue;
            }

            // sums colored and uncolored neighbors for this node
            let (colored, uncolored) = node
                .edges
                .borrow()
                .iter()
                .filter_map(|i| nodes.get(*i))
                .fold((0, 0), |(mut c, mut u), x| {
                    x.color
                        .get()
                        .and_then(|v| {
                            c += 1;
                            Some(v)
                        })
                        .or_else(|| {
                            u += 1;
                            None
                        });
                    (c, u)
                });

            if (colored > candidate_colored)
                || ((colored == candidate_colored) && (uncolored > candidate_uncolored))
            {
                // this is our new candidate
                candidate_colored = colored;
                candidate_uncolored = uncolored;
                candidate = Some(node);
                continue;
            } else {
                // if we have no candidate at all, pick this one
                if candidate.is_none() {
                    candidate = Some(node);
                }
                continue;
            }
        }
        candidate
    }

    pub fn build(mut self) -> ConflictGraph<K, V, Built<K, V>> {
        self.color();

        let mut buckets = HashMap::new();

        // destructively iterate the state and fill up our buckets
        for node in self.state.0.into_iter() {
            let color = node
                .color
                .get()
                .expect("please report this bug - all nodes must be colored");

            // we're wrapping kv in an Option to use the take().unwrap()
            // methods to bypass a deficiency with borrow-checking in the
            // entry API
            let mut kv = Some((node.key, node.val));

            buckets
                .entry(color.as_usize())
                .and_modify(|e: &mut Vec<(K, V)>| e.push(kv.take().unwrap()))
                .or_insert_with(|| vec![kv.take().unwrap()]);
        }

        ConflictGraph {
            state: Built { buckets },
            _k: PhantomData,
            _v: PhantomData,
        }
    }
}

impl<'g, K, V> ConflictGraph<K, V, Built<K, V>>
where
    V: Dependent,
{
    /// Returns an iterator over separate collections of non-conflicting items
    pub fn iter(&self) -> impl Iterator<Item = &Vec<(K, V)>> {
        self.state.buckets.values()
    }
}

impl<K, V> IntoIterator for ConflictGraph<K, V, Built<K, V>>
where
    V: Dependent,
{
    type Item = Vec<(K, V)>;
    type IntoIter = IntoValues<usize, Vec<(K, V)>>;

    fn into_iter(self) -> Self::IntoIter {
        self.state.buckets.into_values()
    }
}

pub struct ConflictGraphIter<'g, K, T> {
    colors: &'g HashMap<K, T>,
}

impl<'g, K, T> Iterator for ConflictGraphIter<'g, K, T>
where
    T: 'g,
    K: 'g,
{
    type Item = &'g [(K, T)];

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

#[cfg(test)]
mod test {
    use super::{ConflictGraph, Dependent};

    #[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
    enum Dep {
        Blue,
        Yellow,
        Green,
        Cyan,
        White,
    }

    #[derive(Debug)]
    struct Consumer {
        r: Vec<Dep>,
        w: Vec<Dep>,
    }
    impl Dependent for Consumer {
        type Dependency = Dep;

        fn dependencies(&self) -> impl Iterator<Item = Self::Dependency> {
            self.r.clone().into_iter()
        }

        fn exclusive_dependencies(&self) -> impl Iterator<Item = Self::Dependency> {
            self.w.clone().into_iter()
        }
    }

    #[test]
    fn build_conflict_graph() {
        let resources = [
            Consumer {
                r: vec![Dep::Cyan],
                w: vec![Dep::Blue],
            },
            Consumer {
                r: vec![Dep::Cyan],
                w: vec![Dep::Yellow],
            },
            Consumer {
                r: vec![Dep::Cyan],
                w: vec![Dep::Green],
            },
            Consumer {
                r: vec![Dep::Cyan, Dep::White],
                w: vec![Dep::Blue],
            },
            Consumer {
                r: vec![],
                w: vec![Dep::White],
            },
            Consumer {
                r: vec![],
                w: vec![Dep::Yellow],
            },
            Consumer {
                r: vec![Dep::Cyan],
                w: vec![],
            },
            Consumer {
                r: vec![Dep::Blue],
                w: vec![],
            },
            Consumer {
                r: vec![Dep::Cyan, Dep::Blue],
                w: vec![],
            },
            Consumer {
                r: vec![Dep::Blue, Dep::Green],
                w: vec![],
            },
            Consumer {
                r: vec![Dep::Green],
                w: vec![],
            },
            Consumer {
                r: vec![Dep::Yellow],
                w: vec![],
            },
        ];

        let mut graph = ConflictGraph::new();

        for (index, resource) in resources.into_iter().enumerate() {
            graph.insert(index, resource);
        }

        let _conflict_free = graph.build();

        //for (index, bucket) in conflict_free.iter().enumerate() {
        //    println!("bucket: {}", index);
        //    for (i, consumer) in bucket {
        //        let s_writes = format!("*{:?}*", consumer.w);
        //        println!("\t\t{:3}:{:24}{:?}", i, s_writes, consumer.r);
        //    }
        //}
    }
}
