use crate::data_structure::ref_store::{
    ArenaStoreFactory, LayeredArenaStoreFactory, LayeredRef, RcStoreFactory, RefMapper, RefStore,
    RefStoreFactory, RefStoreMut,
};
use crate::traits::monoid::Monoid;
use crate::traits::semigroup::Semigroup;

use std::{
    cmp::{max, min},
    marker::PhantomData,
    mem::size_of,
    ops::Range,
};

pub const SEG_TREE_CACHE_LINE_BYTES: usize = 64;
pub const DEFAULT_SEG_LEAF_BLOCK_CAPACITY: usize = 16;

/// 按目标字节数估算 `SegBlock<V, B>` 能容纳多少个叶值。
///
/// 和 FingerTree 的 chunk 一样，stable Rust 不能把这个依赖 `V` 大小的结果
/// 自动作为默认 const 泛型参数；所以 `SegTree` 保留 `const B`，调用侧可以用这个
/// 函数生成 `B`。
pub const fn seg_leaf_block_capacity_for_bytes<V>(target_bytes: usize) -> usize {
    let item = size_of::<V>();
    let payload = target_bytes.saturating_sub(size_of::<V>());

    if item == 0 {
        if target_bytes == 0 {
            1
        } else {
            target_bytes
        }
    } else {
        let capacity = payload / item;
        if capacity == 0 {
            1
        } else {
            capacity
        }
    }
}

/// 估算一个分支节点的载荷大小。`Ref` 用来描述不同 store 的引用宽度。
pub const fn seg_branch_payload_bytes<V, M, Ref>() -> usize {
    size_of::<usize>() + size_of::<M>() + size_of::<V>() + size_of::<Ref>() * 2
}

/// 同时考虑分支节点大小和目标字节数，估算叶块容量。
///
/// 如果 modifier 或引用类型很大，分支节点可能已经超过一个 cache line；这时叶块
/// 至少按分支节点的大小来估算，避免叶块比相邻分支小太多。
pub const fn seg_leaf_block_capacity_for_ref_bytes<V, M, Ref>(target_bytes: usize) -> usize {
    let branch = seg_branch_payload_bytes::<V, M, Ref>();
    let target = if target_bytes < branch {
        branch
    } else {
        target_bytes
    };
    seg_leaf_block_capacity_for_bytes::<V>(target)
}

/// 结合 `V/M/Ref` 的大小给出一个偏保守的叶块容量。
///
/// 这里让叶块目标大小至少覆盖一个 cache line，并且略大于相邻分支节点。这样在
/// modifier 较大时不会把叶块算得过小；在常见小 modifier 场景下仍然保持较紧凑的块。
pub const fn seg_leaf_block_capacity_for_ref<V, M, Ref>() -> usize {
    let branch = seg_branch_payload_bytes::<V, M, Ref>() + SEG_TREE_CACHE_LINE_BYTES / 4;
    let target = if branch < SEG_TREE_CACHE_LINE_BYTES {
        SEG_TREE_CACHE_LINE_BYTES
    } else {
        branch
    };
    seg_leaf_block_capacity_for_bytes::<V>(target)
}

/// 线段树懒标记。
///
/// `apply` 会直接修改目标值，便于原址更新路径避免克隆整个 `V`。语义上要求：
/// 对任意 `a/b`，先合并后应用标记，等价于分别应用标记后再合并。
pub trait Applier<V: Semigroup> {
    fn apply(&self, to: &mut V);

    /// 持久化路径仍需要得到一个新值；这里集中保留“克隆后原址应用”的默认实现。
    fn applied(&self, mut to: V) -> V {
        self.apply(&mut to);
        to
    }
}

impl<A: Semigroup, B: Semigroup, MA: Applier<A>, MB: Applier<B>> Applier<(A, B)> for (MA, MB) {
    fn apply(&self, (a, b): &mut (A, B)) {
        let (ma, mb) = self;
        ma.apply(a);
        mb.apply(b);
    }
}

impl<A: Semigroup, M: Applier<A>> Applier<A> for Option<M> {
    fn apply(&self, to: &mut A) {
        if let Some(f) = self {
            f.apply(to);
        }
    }
}

type SegRef<V, M, const B: usize, R> = <R as RefStore<SegNode<V, M, B, R>>>::Ref;
type StoreNode<V, M, F, const B: usize> = SegNode<V, M, B, SegTreeStore<V, M, F, B>>;
type InnerStore<V, M, F, const B: usize> = <F as RefStoreFactory>::Store<StoreNode<V, M, F, B>>;
type BaseArenaSegTreeStore<'base, V, M, const B: usize> =
    SegTreeStore<V, M, ArenaStoreFactory<'base>, B>;

pub type LayeredArenaSegTreeStore<
    'store,
    'base,
    'scratch,
    V,
    M,
    const B: usize = DEFAULT_SEG_LEAF_BLOCK_CAPACITY,
> = SegTreeStore<
    V,
    M,
    LayeredArenaStoreFactory<
        'store,
        'scratch,
        BaseArenaSegTreeStore<'base, V, M, B>,
        SegTreeLayerMapper<'store, 'base, 'scratch, V, M, B>,
    >,
    B,
>;

type SegTreeLayerMarker<'store, 'base, 'scratch, V, M, const B: usize> =
    PhantomData<fn() -> (&'store (), &'base (), &'scratch (), V, M, [(); B])>;

#[doc(hidden)]
pub struct SegTreeLayerMapper<'store, 'base, 'scratch, V, M, const B: usize>
where
    V: Semigroup + Clone,
    M: Clone,
{
    marker: SegTreeLayerMarker<'store, 'base, 'scratch, V, M, B>,
}

pub struct SegTreeStore<V, M, F = RcStoreFactory, const B: usize = DEFAULT_SEG_LEAF_BLOCK_CAPACITY>
where
    V: Semigroup + Clone,
    M: Clone,
    F: RefStoreFactory,
    InnerStore<V, M, F, B>: RefStore<StoreNode<V, M, F, B>>,
{
    nodes: InnerStore<V, M, F, B>,
}

impl<V, M, F, const B: usize> SegTreeStore<V, M, F, B>
where
    V: Semigroup + Clone,
    M: Clone,
    F: RefStoreFactory,
    InnerStore<V, M, F, B>: RefStore<StoreNode<V, M, F, B>>,
{
    pub fn new(factory: F) -> Self {
        Self {
            nodes: factory.store(),
        }
    }
}

impl<V, M, F, const B: usize> Default for SegTreeStore<V, M, F, B>
where
    V: Semigroup + Clone,
    M: Clone,
    F: RefStoreFactory + Default,
    InnerStore<V, M, F, B>: RefStore<StoreNode<V, M, F, B>>,
{
    fn default() -> Self {
        Self::new(F::default())
    }
}

impl<'base, V, M, const B: usize> SegTreeStore<V, M, ArenaStoreFactory<'base>, B>
where
    V: Semigroup + Clone,
    M: Clone,
{
    pub fn layered<'store, T, F>(&'store self, capacity: usize, f: F) -> T
    where
        F: for<'scratch> FnOnce(LayeredArenaSegTreeStore<'store, 'base, 'scratch, V, M, B>) -> T,
    {
        f(SegTreeStore::new(LayeredArenaStoreFactory::new(
            self, capacity,
        )))
    }
}

impl<'store, 'base, 'scratch, V, M, const B: usize>
    LayeredArenaSegTreeStore<'store, 'base, 'scratch, V, M, B>
where
    V: Semigroup + Clone,
    M: Clone,
{
    pub fn from_base(
        &self,
        tree: &SegTree<V, M, B, SegTreeStore<V, M, ArenaStoreFactory<'base>, B>>,
    ) -> SegTree<V, M, B, Self> {
        tree.map_refs(&|reference| LayeredRef::Base(*reference))
    }
}

impl<'store, 'base, 'scratch, V, M, const B: usize>
    RefMapper<SegNode<V, M, B, LayeredArenaSegTreeStore<'store, 'base, 'scratch, V, M, B>>>
    for SegTreeLayerMapper<'store, 'base, 'scratch, V, M, B>
where
    V: Semigroup + Clone,
    M: Clone,
{
    type Source = SegNode<V, M, B, SegTreeStore<V, M, ArenaStoreFactory<'base>, B>>;

    fn map_ref(
        value: &Self::Source,
    ) -> SegNode<V, M, B, LayeredArenaSegTreeStore<'store, 'base, 'scratch, V, M, B>> {
        value.map_refs(&|reference| LayeredRef::Base(*reference))
    }
}

impl<V, M, S, const B: usize> RefStore<StoreNode<V, M, S, B>> for SegTreeStore<V, M, S, B>
where
    V: Semigroup + Clone,
    M: Clone,
    S: RefStoreFactory,
    InnerStore<V, M, S, B>: RefStore<StoreNode<V, M, S, B>>,
{
    type Ref = <InnerStore<V, M, S, B> as RefStore<StoreNode<V, M, S, B>>>::Ref;

    fn alloc(&mut self, value: StoreNode<V, M, S, B>) -> Self::Ref {
        self.nodes.alloc(value)
    }

    fn with_ref<T, C>(&self, reference: &Self::Ref, f: C) -> T
    where
        C: FnOnce(&StoreNode<V, M, S, B>) -> T,
    {
        self.nodes.with_ref(reference, f)
    }
}

impl<V, M, S, const B: usize> RefStoreMut<StoreNode<V, M, S, B>> for SegTreeStore<V, M, S, B>
where
    V: Semigroup + Clone,
    M: Clone,
    S: RefStoreFactory,
    InnerStore<V, M, S, B>: RefStoreMut<StoreNode<V, M, S, B>>,
{
    fn set_ref(&mut self, reference: &Self::Ref, value: SegNode<V, M, B, Self>) {
        self.nodes.set_ref(reference, value);
    }

    fn replace_ref(
        &mut self,
        reference: &Self::Ref,
        value: SegNode<V, M, B, Self>,
    ) -> SegNode<V, M, B, Self> {
        self.nodes.replace_ref(reference, value)
    }
}

enum IterEntry<V, M, const B: usize, R>
where
    V: Semigroup + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
    PendingTree(M, usize, SegNode<V, M, B, R>),
    PendingValue(V),
}

pub struct Iter<
    'a,
    V,
    M,
    const B: usize = DEFAULT_SEG_LEAF_BLOCK_CAPACITY,
    R = SegTreeStore<V, M, RcStoreFactory, B>,
> where
    V: Semigroup + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
    stack: Vec<IterEntry<V, M, B, R>>,
    refs: &'a R,
}

impl<V, M, const B: usize, R> Iterator for Iter<'_, V, M, B, R>
where
    V: Semigroup + Clone,
    M: Applier<V> + Monoid + Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
    type Item = V;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(entry) = self.stack.pop() {
            let (prefix, size, tree) = match entry {
                IterEntry::PendingValue(value) => return Some(value),
                IterEntry::PendingTree(prefix, size, tree) => (prefix, size, tree),
            };
            match tree {
                SegNode::Block(block) => {
                    for value in block.into_units(size).into_iter().rev() {
                        self.stack
                            .push(IterEntry::PendingValue(prefix.clone().applied(value)));
                    }
                }
                SegNode::Branch {
                    left_size,
                    modifier,
                    right,
                    left,
                    ..
                } => {
                    let prefix = prefix.merge(modifier);
                    let mid = left_size;
                    let right = self.refs.with_ref(&right, |node| node.clone());
                    let left = self.refs.with_ref(&left, |node| node.clone());
                    self.stack
                        .push(IterEntry::PendingTree(prefix.clone(), size - mid, right));
                    self.stack.push(IterEntry::PendingTree(prefix, mid, left));
                }
                SegNode::Empty => (),
            };
        }
        None
    }
}

pub struct SegTree<
    V,
    M,
    const B: usize = DEFAULT_SEG_LEAF_BLOCK_CAPACITY,
    R = SegTreeStore<V, M, RcStoreFactory, B>,
> where
    V: Semigroup + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
    len: usize,
    root: SegNode<V, M, B, R>,
}

#[doc(hidden)]
pub struct SegBlock<V, const N: usize>
where
    V: Semigroup + Clone,
{
    value: V,
    items: [V; N],
}

pub enum SegNode<
    V,
    M,
    const B: usize = DEFAULT_SEG_LEAF_BLOCK_CAPACITY,
    R = SegTreeStore<V, M, RcStoreFactory, B>,
> where
    V: Semigroup + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
    Empty,
    Block(SegBlock<V, B>),
    Branch {
        left_size: usize,
        modifier: M,
        value: V,
        left: SegRef<V, M, B, R>,
        right: SegRef<V, M, B, R>,
    },
}

impl<V, M, const B: usize, R> Clone for SegTree<V, M, B, R>
where
    V: Semigroup + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
    fn clone(&self) -> Self {
        Self {
            len: self.len,
            root: self.root.clone(),
        }
    }
}

impl<V, M, const B: usize, R> Clone for SegNode<V, M, B, R>
where
    V: Semigroup + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
    fn clone(&self) -> Self {
        match self {
            Self::Empty => Self::Empty,
            Self::Block(block) => Self::Block(block.clone()),
            Self::Branch {
                left_size,
                modifier,
                value,
                left,
                right,
            } => Self::Branch {
                left_size: *left_size,
                modifier: modifier.clone(),
                value: value.clone(),
                left: left.clone(),
                right: right.clone(),
            },
        }
    }
}

impl<V, M, const B: usize, R> SegTree<V, M, B, R>
where
    V: Semigroup + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
    fn map_refs<S, F>(&self, ref_map: &F) -> SegTree<V, M, B, S>
    where
        S: RefStore<SegNode<V, M, B, S>>,
        F: Fn(&SegRef<V, M, B, R>) -> SegRef<V, M, B, S>,
    {
        SegTree {
            len: self.len,
            root: self.root.map_refs(ref_map),
        }
    }
}

impl<V, M, const B: usize, R> SegNode<V, M, B, R>
where
    V: Semigroup + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
    fn map_refs<S, F>(&self, ref_map: &F) -> SegNode<V, M, B, S>
    where
        S: RefStore<SegNode<V, M, B, S>>,
        F: Fn(&SegRef<V, M, B, R>) -> SegRef<V, M, B, S>,
    {
        match self {
            Self::Empty => SegNode::Empty,
            Self::Block(block) => SegNode::Block(block.clone()),
            Self::Branch {
                left_size,
                modifier,
                value,
                left,
                right,
            } => SegNode::Branch {
                left_size: *left_size,
                modifier: modifier.clone(),
                value: value.clone(),
                left: ref_map(left),
                right: ref_map(right),
            },
        }
    }
}

impl<V, const N: usize> SegBlock<V, N>
where
    V: Semigroup + Clone,
{
    fn as_slice(&self, size: usize) -> &[V] {
        debug_assert!(size <= N);
        &self.items[..size]
    }

    fn as_mut_slice(&mut self, size: usize) -> &mut [V] {
        debug_assert!(size <= N);
        &mut self.items[..size]
    }

    fn range_bounds(&self, size: usize, range: Range<usize>) -> Range<usize> {
        debug_assert!(size <= N);
        min(range.start, size)..min(range.end, size)
    }
}

impl<V, const N: usize> SegBlock<V, N>
where
    V: Monoid + Clone,
{
    fn build<F: Fn(usize) -> V>(offset: usize, len: usize, init: F) -> Self {
        assert!(N > 0, "seg tree block capacity must be positive");
        assert!(len <= N, "seg tree block capacity exceeded");
        // 所有槽位都由 safe Rust 初始化：有效前缀来自调用方的 `init`，
        // 未使用后缀来自 `V::empty()`，因此槽位大小仍然就是 `V`。
        let items = std::array::from_fn(|i| {
            if i < len {
                init(offset + i)
            } else {
                V::empty()
            }
        });
        let value = items[..len]
            .iter()
            .cloned()
            .reduce(Semigroup::merge)
            .unwrap_or_else(V::empty);
        Self { value, items }
    }

    fn all(&self, size: usize) -> V {
        debug_assert!(size <= N);
        self.value.clone()
    }
}

impl<V, const N: usize> SegBlock<V, N>
where
    V: Semigroup + Clone,
{
    fn into_units(self, size: usize) -> Vec<V> {
        self.items.into_iter().take(size).collect()
    }
}

impl<V, const N: usize> SegBlock<V, N>
where
    V: Monoid + Clone,
{
    fn apply_all_mut<M>(&mut self, size: usize, m: &M)
    where
        M: Clone + Monoid + Applier<V>,
    {
        if m.is_empty() {
            return;
        }
        for value in self.as_mut_slice(size) {
            m.apply(value);
        }
        m.apply(&mut self.value);
    }
}

impl<V, const N: usize> Clone for SegBlock<V, N>
where
    V: Semigroup + Clone,
{
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            items: self.items.clone(),
        }
    }
}

impl<V, const N: usize> SegBlock<V, N>
where
    V: Monoid + Clone,
{
    fn query(&self, size: usize, range: Range<usize>) -> V {
        let range = self.range_bounds(size, range);
        if range.is_empty() {
            return V::empty();
        }
        if range.start == 0 && range.end == size {
            return self.all(size);
        }
        self.as_slice(size)[range]
            .iter()
            .cloned()
            .reduce(Semigroup::merge)
            .unwrap_or_else(V::empty)
    }

    fn apply_mut<M>(&mut self, size: usize, range: Range<usize>, m: &M)
    where
        M: Applier<V> + Monoid + Clone,
    {
        if m.is_empty() {
            return;
        }
        let range = self.range_bounds(size, range);
        if range.is_empty() {
            return;
        }
        if range.start == 0 && range.end == size {
            self.apply_all_mut(size, m);
            return;
        }

        let slice = self.as_mut_slice(size);
        let (before, rest) = slice.split_at_mut(range.start);
        let (middle, after) = rest.split_at_mut(range.end - range.start);
        let mut result: Option<V> = None;

        for value in before {
            let value = value.clone();
            result = Some(match result {
                Some(result) => result.merge(value),
                None => value,
            });
        }
        for value in middle {
            m.apply(value);
            let value = value.clone();
            result = Some(match result {
                Some(result) => result.merge(value),
                None => value,
            });
        }
        for value in after {
            let value = value.clone();
            result = Some(match result {
                Some(result) => result.merge(value),
                None => value,
            });
        }

        let value = result.unwrap_or_else(V::empty);
        self.value = value;
    }
}

impl<V, M, const B: usize, R> SegTree<V, M, B, R>
where
    V: Monoid + Clone,
    M: Applier<V> + Monoid + Clone,
    R: RefStore<SegNode<V, M, B, R>> + Default,
{
    pub fn build<F: Fn(usize) -> V + Clone>(len: usize, init: F) -> Self {
        let mut refs = R::default();
        Self::build_in(&mut refs, len, init)
    }
}

impl<V, M, const B: usize, R> SegTree<V, M, B, R>
where
    V: Monoid + Clone,
    M: Applier<V> + Monoid + Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
    pub fn build_in<F: Fn(usize) -> V + Clone>(refs: &mut R, len: usize, init: F) -> Self {
        assert!(B > 0, "seg tree leaf block capacity must be positive");
        let root = SegNode::build_inner(refs, 0, len, init);
        Self { len, root }
    }

    pub fn iter<'a>(&self, refs: &'a R) -> Iter<'a, V, M, B, R> {
        Iter {
            stack: vec![IterEntry::PendingTree(
                M::empty(),
                self.len,
                self.root.clone(),
            )],
            refs,
        }
    }

    pub fn query(&self, refs: &R, range: Range<usize>) -> V {
        self.root.query(refs, self.len, range)
    }

    pub fn apply(&self, refs: &mut R, range: Range<usize>, m: &M) -> Self {
        Self {
            len: self.len,
            root: self.root.apply(refs, self.len, range, m),
        }
    }
}

impl<V, M, const B: usize, R> SegTree<V, M, B, R>
where
    V: Monoid + Clone,
    M: Applier<V> + Monoid + Clone,
    R: RefStore<SegNode<V, M, B, R>> + RefStoreMut<SegNode<V, M, B, R>>,
{
    pub fn apply_mut(&mut self, refs: &mut R, range: Range<usize>, m: &M) {
        self.root.apply_mut(refs, self.len, range, m);
    }
}

impl<V, M, const B: usize, R> SegNode<V, M, B, R>
where
    V: Monoid + Clone,
    M: Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
    fn all(&self, size: usize) -> V {
        match self {
            Self::Empty => V::empty(),
            Self::Block(block) => block.all(size),
            Self::Branch { value, .. } => value.clone(),
        }
    }
}

impl<V, M, const B: usize, R> SegNode<V, M, B, R>
where
    V: Monoid + Clone,
    M: Clone + Monoid + Applier<V>,
    R: RefStore<SegNode<V, M, B, R>>,
{
    fn apply_all_mut(&mut self, size: usize, m: &M) {
        if m.is_empty() {
            return;
        }
        match self {
            Self::Empty => {}
            Self::Block(block) => block.apply_all_mut(size, m),
            Self::Branch {
                modifier, value, ..
            } => {
                m.apply(value);
                *modifier = m.clone().merge(modifier.clone());
            }
        }
    }
}

impl<V, M, const B: usize, R> SegNode<V, M, B, R>
where
    V: Monoid + Clone,
    M: Monoid + Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
    fn build_inner<F: Fn(usize) -> V + Clone>(
        refs: &mut R,
        offset: usize,
        len: usize,
        init: F,
    ) -> Self {
        if len == 0 {
            Self::Empty
        } else if len <= B {
            Self::Block(SegBlock::build(offset, len, init))
        } else {
            let left_size = Self::split_left_size(len);
            let left = Self::build_inner(refs, offset, left_size, init.clone());
            let right = Self::build_inner(refs, offset + left_size, len - left_size, init);
            let value = left.all(left_size).merge(right.all(len - left_size));

            Self::Branch {
                left_size,
                modifier: M::empty(),
                value,
                left: refs.alloc(left),
                right: refs.alloc(right),
            }
        }
    }

    fn split_left_size(len: usize) -> usize {
        let blocks = len.div_ceil(B);
        let left_blocks = blocks / 2;
        debug_assert!(left_blocks > 0);
        let left_size = left_blocks * B;
        debug_assert!(0 < left_size && left_size < len);
        left_size
    }
}

impl<V, M, const B: usize, R> SegNode<V, M, B, R>
where
    V: Monoid + Clone,
    M: Applier<V> + Monoid + Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
    fn query(&self, refs: &R, size: usize, range: Range<usize>) -> V {
        match self {
            Self::Empty => V::empty(),
            Self::Block(block) => block.query(size, range),
            Self::Branch {
                left_size,
                modifier,
                value,
                left,
                right,
            } => {
                if range.start == 0 && size <= range.end {
                    value.clone()
                } else {
                    let mid = *left_size;

                    let value = if range.end <= mid {
                        refs.with_ref(left, |node| node.query(refs, mid, range))
                    } else if mid <= range.start {
                        refs.with_ref(right, |node| {
                            node.query(refs, size - mid, range.start - mid..range.end - mid)
                        })
                    } else {
                        V::merge(
                            refs.with_ref(left, |node| node.query(refs, mid, range.start..mid)),
                            refs.with_ref(right, |node| {
                                node.query(refs, size - mid, 0..range.end - mid)
                            }),
                        )
                    };
                    if modifier.is_empty() {
                        value
                    } else {
                        modifier.applied(value)
                    }
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
    fn apply(&self, refs: &mut R, size: usize, range: Range<usize>, m: &M) -> Self {
        let mut node = self.clone();
        node.apply_owned_mut(refs, size, range, m);
        node
    }

    fn apply_owned_mut(&mut self, refs: &mut R, size: usize, range: Range<usize>, m: &M) {
        if m.is_empty() || range.is_empty() {
            return;
        }
        match self {
            Self::Empty => {}
            Self::Block(block) => block.apply_mut(size, range, m),
            Self::Branch {
                left_size,
                modifier,
                value,
                left,
                right,
            } => {
                if range.start == 0 && size <= range.end {
                    m.apply(value);
                    *modifier = m.clone().merge(modifier.clone());
                    return;
                }

                let mid = *left_size;
                let right_size = size - mid;
                let push_modifier = !modifier.is_empty();
                let left_range = (range.start < mid).then(|| range.start..min(range.end, mid));
                let right_range =
                    (mid < range.end).then(|| max(range.start, mid) - mid..range.end - mid);
                let old_left = left.clone();
                let old_right = right.clone();

                let (left_ref, left_value) = if push_modifier || left_range.is_some() {
                    let mut left_node = refs.with_ref(&old_left, |node| node.clone());
                    if push_modifier {
                        left_node.apply_all_mut(mid, modifier);
                    }
                    if let Some(range) = left_range {
                        left_node.apply_owned_mut(refs, mid, range, m);
                    }
                    let value = left_node.all(mid);
                    (refs.alloc(left_node), value)
                } else {
                    (old_left, refs.with_ref(left, |node| node.all(mid)))
                };

                let (right_ref, right_value) = if push_modifier || right_range.is_some() {
                    let mut right_node = refs.with_ref(&old_right, |node| node.clone());
                    if push_modifier {
                        right_node.apply_all_mut(right_size, modifier);
                    }
                    if let Some(range) = right_range {
                        right_node.apply_owned_mut(refs, right_size, range, m);
                    }
                    let value = right_node.all(right_size);
                    (refs.alloc(right_node), value)
                } else {
                    (old_right, refs.with_ref(right, |node| node.all(right_size)))
                };

                *modifier = M::empty();
                *value = left_value.merge(right_value);
                *left = left_ref;
                *right = right_ref;
            }
        }
    }

    fn apply_mut(&mut self, refs: &mut R, size: usize, range: Range<usize>, m: &M)
    where
        R: RefStoreMut<SegNode<V, M, B, R>>,
    {
        if m.is_empty() {
            return;
        }
        match self {
            Self::Empty => {}
            Self::Block(block) => block.apply_mut(size, range, m),
            Self::Branch {
                left_size,
                modifier,
                value,
                left,
                right,
            } => {
                if range.start == 0 && size <= range.end {
                    m.apply(value);
                    *modifier = m.clone().merge(modifier.clone());
                } else {
                    let mid = *left_size;
                    let left_ref = left.clone();
                    let right_ref = right.clone();
                    let push_modifier = !modifier.is_empty();

                    let mut left_node = refs.replace_ref(&left_ref, Self::Empty);
                    if push_modifier {
                        left_node.apply_all_mut(mid, modifier);
                    }
                    if range.start < mid {
                        left_node.apply_mut(refs, mid, range.start..min(range.end, mid), m);
                    }
                    refs.set_ref(&left_ref, left_node);

                    let right_size = size - mid;
                    let mut right_node = refs.replace_ref(&right_ref, Self::Empty);
                    if push_modifier {
                        right_node.apply_all_mut(right_size, modifier);
                    }
                    if mid < range.end {
                        right_node.apply_mut(
                            refs,
                            right_size,
                            max(range.start, mid) - mid..range.end - mid,
                            m,
                        );
                    }
                    refs.set_ref(&right_ref, right_node);

                    let left_value = refs.with_ref(&left_ref, |node| node.all(mid));
                    let right_value = refs.with_ref(&right_ref, |node| node.all(size - mid));
                    *modifier = M::empty();
                    *value = left_value.merge(right_value);
                }
            }
        }
    }
}
