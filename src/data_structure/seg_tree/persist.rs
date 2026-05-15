use crate::data_structure::ref_store::{
    ArenaStoreFactory, LayeredArenaStoreFactory, LayeredRef, RcStoreFactory, RefMapper, RefStore,
    RefStoreFactory, RefStoreMut,
};
use crate::traits::monoid::Monoid;
use crate::traits::semigroup::Semigroup;

use std::{
    cmp::{max, min},
    marker::PhantomData,
    ops::Range,
};

/// `m1.apply(a.merge(&b)) == m1.apply(a).merge(&m1.apply(b))`
pub trait Applier<V: Semigroup> {
    fn apply(&self, to: V) -> V;
}

impl<A: Semigroup, B: Semigroup, MA: Applier<A>, MB: Applier<B>> Applier<(A, B)> for (MA, MB) {
    fn apply(&self, (a, b): (A, B)) -> (A, B) {
        let (ma, mb) = self;
        (ma.apply(a), mb.apply(b))
    }
}

impl<A: Semigroup, M: Applier<A>> Applier<A> for Option<M> {
    fn apply(&self, to: A) -> A {
        match self {
            None => to,
            Some(f) => f.apply(to),
        }
    }
}

type SegRef<V, M, R> = <R as RefStore<SegNode<V, M, R>>>::Ref;
type StoreNode<V, M, F> = SegNode<V, M, SegTreeStore<V, M, F>>;
type InnerStore<V, M, F> = <F as RefStoreFactory>::Store<StoreNode<V, M, F>>;
type BaseArenaSegTreeStore<'base, V, M> = SegTreeStore<V, M, ArenaStoreFactory<'base>>;

pub type LayeredArenaSegTreeStore<'store, 'base, 'scratch, V, M> = SegTreeStore<
    V,
    M,
    LayeredArenaStoreFactory<
        'store,
        'scratch,
        BaseArenaSegTreeStore<'base, V, M>,
        SegTreeLayerMapper<'store, 'base, 'scratch, V, M>,
    >,
>;

type SegTreeLayerMarker<'store, 'base, 'scratch, V, M> =
    PhantomData<fn() -> (&'store (), &'base (), &'scratch (), V, M)>;

#[doc(hidden)]
pub struct SegTreeLayerMapper<'store, 'base, 'scratch, V, M>
where
    V: Semigroup + Clone,
    M: Clone,
{
    marker: SegTreeLayerMarker<'store, 'base, 'scratch, V, M>,
}

pub struct SegTreeStore<V, M, F = RcStoreFactory>
where
    V: Semigroup + Clone,
    M: Clone,
    F: RefStoreFactory,
    InnerStore<V, M, F>: RefStore<StoreNode<V, M, F>>,
{
    nodes: InnerStore<V, M, F>,
}

impl<V, M, F> SegTreeStore<V, M, F>
where
    V: Semigroup + Clone,
    M: Clone,
    F: RefStoreFactory,
    InnerStore<V, M, F>: RefStore<StoreNode<V, M, F>>,
{
    pub fn new(factory: F) -> Self {
        Self {
            nodes: factory.store(),
        }
    }
}

impl<V, M, F> Default for SegTreeStore<V, M, F>
where
    V: Semigroup + Clone,
    M: Clone,
    F: RefStoreFactory + Default,
    InnerStore<V, M, F>: RefStore<StoreNode<V, M, F>>,
{
    fn default() -> Self {
        Self::new(F::default())
    }
}

impl<'base, V, M> SegTreeStore<V, M, ArenaStoreFactory<'base>>
where
    V: Semigroup + Clone,
    M: Clone,
{
    pub fn layered<'store, T, F>(&'store self, capacity: usize, f: F) -> T
    where
        F: for<'scratch> FnOnce(LayeredArenaSegTreeStore<'store, 'base, 'scratch, V, M>) -> T,
    {
        f(SegTreeStore::new(LayeredArenaStoreFactory::new(
            self, capacity,
        )))
    }
}

impl<'store, 'base, 'scratch, V, M> LayeredArenaSegTreeStore<'store, 'base, 'scratch, V, M>
where
    V: Semigroup + Clone,
    M: Clone,
{
    pub fn from_base(
        &self,
        tree: &SegTree<V, M, SegTreeStore<V, M, ArenaStoreFactory<'base>>>,
    ) -> SegTree<V, M, Self> {
        tree.map_refs(&|reference| LayeredRef::Base(*reference))
    }
}

impl<'store, 'base, 'scratch, V, M>
    RefMapper<SegNode<V, M, LayeredArenaSegTreeStore<'store, 'base, 'scratch, V, M>>>
    for SegTreeLayerMapper<'store, 'base, 'scratch, V, M>
where
    V: Semigroup + Clone,
    M: Clone,
{
    type Source = SegNode<V, M, SegTreeStore<V, M, ArenaStoreFactory<'base>>>;

    fn map_ref(
        value: &Self::Source,
    ) -> SegNode<V, M, LayeredArenaSegTreeStore<'store, 'base, 'scratch, V, M>> {
        value.map_refs(&|reference| LayeredRef::Base(*reference))
    }
}

impl<V, M, S> RefStore<StoreNode<V, M, S>> for SegTreeStore<V, M, S>
where
    V: Semigroup + Clone,
    M: Clone,
    S: RefStoreFactory,
    InnerStore<V, M, S>: RefStore<StoreNode<V, M, S>>,
{
    type Ref = <InnerStore<V, M, S> as RefStore<StoreNode<V, M, S>>>::Ref;

    fn alloc(&mut self, value: StoreNode<V, M, S>) -> Self::Ref {
        self.nodes.alloc(value)
    }

    fn with_ref<T, C>(&self, reference: &Self::Ref, f: C) -> T
    where
        C: FnOnce(&StoreNode<V, M, S>) -> T,
    {
        self.nodes.with_ref(reference, f)
    }
}

impl<V, M, S> RefStoreMut<StoreNode<V, M, S>> for SegTreeStore<V, M, S>
where
    V: Semigroup + Clone,
    M: Clone,
    S: RefStoreFactory,
    InnerStore<V, M, S>: RefStoreMut<StoreNode<V, M, S>>,
{
    fn set_ref(&mut self, reference: &Self::Ref, value: SegNode<V, M, Self>) {
        self.nodes.set_ref(reference, value);
    }
}

pub struct Iter<'a, V, M, R = SegTreeStore<V, M>>
where
    V: Semigroup + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, R>>,
{
    stack: Vec<(M, SegNode<V, M, R>)>,
    refs: &'a R,
}

impl<V, M, R> Iterator for Iter<'_, V, M, R>
where
    V: Semigroup + Clone,
    M: Applier<V> + Semigroup + Clone,
    R: RefStore<SegNode<V, M, R>>,
{
    type Item = V;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((prefix, tree)) = self.stack.pop() {
            match tree {
                SegNode::Unit(v) => return Some(prefix.apply(v)),
                SegNode::Branch {
                    modifier,
                    right,
                    left,
                    ..
                } => {
                    let prefix = prefix.merge(modifier);
                    let right = self.refs.with_ref(&right, |node| node.clone());
                    let left = self.refs.with_ref(&left, |node| node.clone());
                    self.stack.push((prefix.clone(), right));
                    self.stack.push((prefix, left));
                }
                SegNode::Empty => (),
            };
        }
        None
    }
}

pub struct SegTree<V, M, R = SegTreeStore<V, M>>
where
    V: Semigroup + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, R>>,
{
    root: SegNode<V, M, R>,
}

pub enum SegNode<V, M, R = SegTreeStore<V, M>>
where
    V: Semigroup + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, R>>,
{
    Empty,
    Unit(V),
    Branch {
        size: usize,
        modifier: M,
        value: V,
        left: SegRef<V, M, R>,
        right: SegRef<V, M, R>,
    },
}

impl<V, M, R> Clone for SegTree<V, M, R>
where
    V: Semigroup + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, R>>,
{
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
        }
    }
}

impl<V, M, R> Clone for SegNode<V, M, R>
where
    V: Semigroup + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, R>>,
{
    fn clone(&self) -> Self {
        match self {
            Self::Empty => Self::Empty,
            Self::Unit(value) => Self::Unit(value.clone()),
            Self::Branch {
                size,
                modifier,
                value,
                left,
                right,
            } => Self::Branch {
                size: *size,
                modifier: modifier.clone(),
                value: value.clone(),
                left: left.clone(),
                right: right.clone(),
            },
        }
    }
}

impl<V, M, R> SegTree<V, M, R>
where
    V: Semigroup + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, R>>,
{
    fn map_refs<S, F>(&self, ref_map: &F) -> SegTree<V, M, S>
    where
        S: RefStore<SegNode<V, M, S>>,
        F: Fn(&SegRef<V, M, R>) -> SegRef<V, M, S>,
    {
        SegTree {
            root: self.root.map_refs(ref_map),
        }
    }
}

impl<V, M, R> SegNode<V, M, R>
where
    V: Semigroup + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, R>>,
{
    fn map_refs<S, F>(&self, ref_map: &F) -> SegNode<V, M, S>
    where
        S: RefStore<SegNode<V, M, S>>,
        F: Fn(&SegRef<V, M, R>) -> SegRef<V, M, S>,
    {
        match self {
            Self::Empty => SegNode::Empty,
            Self::Unit(value) => SegNode::Unit(value.clone()),
            Self::Branch {
                size,
                modifier,
                value,
                left,
                right,
            } => SegNode::Branch {
                size: *size,
                modifier: modifier.clone(),
                value: value.clone(),
                left: ref_map(left),
                right: ref_map(right),
            },
        }
    }
}

impl<V, M, R> SegTree<V, M, R>
where
    V: Monoid + Clone,
    M: Applier<V> + Monoid + Clone,
    R: RefStore<SegNode<V, M, R>> + Default,
{
    pub fn build<F: Fn(usize) -> V + Clone>(len: usize, init: F) -> Self {
        let mut refs = R::default();
        Self::build_in(&mut refs, len, init)
    }
}

impl<V, M, R> SegTree<V, M, R>
where
    V: Monoid + Clone,
    M: Applier<V> + Monoid + Clone,
    R: RefStore<SegNode<V, M, R>>,
{
    pub fn build_in<F: Fn(usize) -> V + Clone>(refs: &mut R, len: usize, init: F) -> Self {
        Self {
            root: SegNode::build_inner(refs, 0, len, init),
        }
    }

    pub fn iter<'a>(&self, refs: &'a R) -> Iter<'a, V, M, R> {
        Iter {
            stack: vec![(M::empty(), self.root.clone())],
            refs,
        }
    }

    pub fn query(&self, refs: &R, range: Range<usize>) -> V {
        self.root.query(refs, range)
    }

    pub fn apply(&self, refs: &mut R, range: Range<usize>, m: M) -> Self {
        Self {
            root: self.root.apply(refs, range, m),
        }
    }
}

impl<V, M, R> SegTree<V, M, R>
where
    V: Monoid + Clone,
    M: Applier<V> + Monoid + Clone,
    R: RefStore<SegNode<V, M, R>> + RefStoreMut<SegNode<V, M, R>>,
{
    pub fn apply_mut(&mut self, refs: &mut R, range: Range<usize>, m: M) {
        self.root.apply_mut(refs, range, m);
    }
}

impl<V, M, R> SegNode<V, M, R>
where
    V: Monoid + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, R>>,
{
    fn all(&self) -> V {
        match self {
            Self::Empty => V::empty(),
            Self::Unit(v) => v.clone(),
            Self::Branch { value, .. } => value.clone(),
        }
    }
}

impl<V, M, R> SegNode<V, M, R>
where
    V: Clone + Semigroup,
    M: Clone + Semigroup + Applier<V>,
    R: RefStore<SegNode<V, M, R>>,
{
    fn apply_all(&self, m: M) -> Self {
        match self {
            Self::Empty => Self::Empty,
            Self::Unit(v) => Self::Unit(m.apply(v.clone())),
            Self::Branch {
                size,
                modifier,
                value,
                left,
                right,
            } => Self::Branch {
                size: *size,
                value: m.clone().apply(value.clone()),
                modifier: m.merge(modifier.clone()),
                left: left.clone(),
                right: right.clone(),
            },
        }
    }

    fn apply_all_mut(&mut self, m: M) {
        match self {
            Self::Empty => {}
            Self::Unit(v) => *v = m.apply(v.clone()),
            Self::Branch {
                modifier, value, ..
            } => {
                *value = m.clone().apply(value.clone());
                *modifier = m.merge(modifier.clone());
            }
        }
    }
}

impl<V, M, R> SegNode<V, M, R>
where
    V: Monoid + Clone,
    M: Monoid + Clone,
    R: RefStore<SegNode<V, M, R>>,
{
    fn build_inner<F: Fn(usize) -> V + Clone>(
        refs: &mut R,
        offset: usize,
        len: usize,
        init: F,
    ) -> Self {
        match len {
            0 => Self::Empty,
            1 => Self::Unit(init(offset)),
            len => {
                let mid = len / 2;
                let left = Self::build_inner(refs, offset, mid, init.clone());
                let right = Self::build_inner(refs, offset + mid, len - mid, init);
                let value = left.all().merge(right.all());

                Self::Branch {
                    size: len,
                    modifier: M::empty(),
                    value,
                    left: refs.alloc(left),
                    right: refs.alloc(right),
                }
            }
        }
    }
}

impl<V, M, R> SegNode<V, M, R>
where
    V: Monoid + Clone,
    M: Applier<V> + Monoid + Clone,
    R: RefStore<SegNode<V, M, R>>,
{
    fn query(&self, refs: &R, range: Range<usize>) -> V {
        match self {
            Self::Empty => V::empty(),
            Self::Unit(v) => {
                if range.contains(&0) {
                    v.clone()
                } else {
                    V::empty()
                }
            }
            Self::Branch {
                size,
                modifier,
                value,
                left,
                right,
            } => {
                if range.start == 0 && *size <= range.end {
                    value.clone()
                } else {
                    let mid = size / 2;

                    modifier.apply(if range.end <= mid {
                        refs.with_ref(left, |node| node.query(refs, range))
                    } else if mid <= range.start {
                        refs.with_ref(right, |node| {
                            node.query(refs, range.start - mid..range.end - mid)
                        })
                    } else {
                        V::merge(
                            refs.with_ref(left, |node| node.query(refs, range.start..mid)),
                            refs.with_ref(right, |node| node.query(refs, 0..range.end - mid)),
                        )
                    })
                }
            }
        }
    }

    /// Apply a modifier to a SegTree.
    ///
    /// # Arguments
    ///
    /// * `range` - The half-open range to update.
    /// * `m` - The modifier to apply.
    fn apply(&self, refs: &mut R, range: Range<usize>, m: M) -> Self {
        match self {
            Self::Empty => Self::Empty,
            Self::Unit(v) => {
                if range.contains(&0) {
                    Self::Unit(m.apply(v.clone()))
                } else {
                    Self::Unit(v.clone())
                }
            }
            Self::Branch {
                size,
                modifier,
                value,
                left,
                right,
            } => {
                if range.start == 0 && *size <= range.end {
                    Self::Branch {
                        size: *size,
                        value: m.clone().apply(value.clone()),
                        modifier: m.merge(modifier.clone()),
                        left: left.clone(),
                        right: right.clone(),
                    }
                } else {
                    let mid = size / 2;
                    // 下推当前节点的懒标记，让新建路径上的 modifier 重新从空元开始。
                    let left = refs.with_ref(left, |node| node.apply_all(modifier.clone()));
                    let right = refs.with_ref(right, |node| node.apply_all(modifier.clone()));

                    let new_left = if range.start < mid {
                        left.apply(refs, range.start..min(range.end, mid), m.clone())
                    } else {
                        left
                    };
                    let new_right = if mid < range.end {
                        right.apply(refs, max(range.start, mid) - mid..range.end - mid, m)
                    } else {
                        right
                    };
                    let value = new_left.all().merge(new_right.all());

                    Self::Branch {
                        size: *size,
                        modifier: M::empty(),
                        value,
                        left: refs.alloc(new_left),
                        right: refs.alloc(new_right),
                    }
                }
            }
        }
    }

    fn apply_mut(&mut self, refs: &mut R, range: Range<usize>, m: M)
    where
        R: RefStoreMut<SegNode<V, M, R>>,
    {
        match self {
            Self::Empty => {}
            Self::Unit(v) => {
                if range.contains(&0) {
                    *v = m.apply(v.clone());
                }
            }
            Self::Branch {
                size,
                modifier,
                value,
                left,
                right,
            } => {
                if range.start == 0 && *size <= range.end {
                    *value = m.clone().apply(value.clone());
                    *modifier = m.merge(modifier.clone());
                } else {
                    let size = *size;
                    let mid = size / 2;
                    let left_ref = left.clone();
                    let right_ref = right.clone();
                    let pushed = modifier.clone();

                    let mut left_node = refs.with_ref(&left_ref, |node| node.clone());
                    left_node.apply_all_mut(pushed.clone());
                    if range.start < mid {
                        left_node.apply_mut(refs, range.start..min(range.end, mid), m.clone());
                    }
                    refs.set_ref(&left_ref, left_node);

                    let mut right_node = refs.with_ref(&right_ref, |node| node.clone());
                    right_node.apply_all_mut(pushed);
                    if mid < range.end {
                        right_node.apply_mut(refs, max(range.start, mid) - mid..range.end - mid, m);
                    }
                    refs.set_ref(&right_ref, right_node);

                    let left_value = refs.with_ref(&left_ref, |node| node.all());
                    let right_value = refs.with_ref(&right_ref, |node| node.all());
                    *modifier = M::empty();
                    *value = left_value.merge(right_value);
                }
            }
        }
    }
}
