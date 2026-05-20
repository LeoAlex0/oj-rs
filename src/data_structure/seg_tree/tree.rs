use crate::data_structure::ref_store::{
    ArenaStoreFactory, LayeredArenaStoreFactory, LayeredRef, RcStoreFactory, RefMapper, RefStore,
    RefStoreFactory, RefStoreMut,
};
use crate::traits::monoid::Monoid;
use crate::traits::semigroup::Semigroup;

use std::{
    cmp::{max, min},
    marker::PhantomData,
    mem::{size_of, MaybeUninit},
    ops::Range,
};

pub const SEG_TREE_CACHE_LINE_BYTES: usize = 64;
pub const DEFAULT_SEG_LEAF_BLOCK_CAPACITY: usize = 16;

const fn capacity_for_linear_size(zero_size: usize, one_size: usize, target_bytes: usize) -> usize {
    let item_size = one_size.saturating_sub(zero_size);

    if item_size == 0 {
        if target_bytes == 0 {
            1
        } else {
            target_bytes
        }
    } else {
        let payload = target_bytes.saturating_sub(zero_size);
        let capacity = payload / item_size;
        if capacity == 0 {
            1
        } else {
            capacity
        }
    }
}

#[allow(dead_code)]
struct SegBranchLayout<V, M, Ref> {
    left_size: usize,
    modifier: M,
    value: V,
    left: Ref,
    right: Ref,
}

#[allow(dead_code)]
enum SegNodeLayout<V, M, Ref, const B: usize>
where
    V: Semigroup + Clone,
{
    Empty,
    Block(SegBlock<V, B>),
    Branch(SegBranchLayout<V, M, Ref>),
}

#[allow(dead_code)]
enum SegBlockNodeLayout<V, M, Ref, const B: usize>
where
    V: Semigroup + Clone,
{
    Empty,
    Block(SegBlock<V, B>),
    // 长度为 0 的数组不贡献载荷大小，但会保留分支载荷的对齐要求。
    Branch([MaybeUninit<SegBranchLayout<V, M, Ref>>; 0]),
}

/// 按目标字节数估算 `SegBlock<V, B>` 能容纳多少个叶值。
///
/// 这是线段树唯一保留的容量策略：用实际 `SegBlock` 布局估算叶块增长量，同时参考
/// `SegNode::Branch` 的实际大小，让叶子节点和相邻分支节点的大小尽量接近。bench
/// 显示这个策略在随机更新上略优于只把 `SegBlock` 塞进一个 cache line 的策略。
///
/// `M` 和 `Ref` 不在 `SegBlock` 内；它们只用于估算分支节点大小。也就是说，`M`
/// 不改变叶块的字段布局，但会影响“叶块应该做多大才和分支节点匹配”这个决策。
pub const fn seg_block_capacity_for_bytes<V, M, Ref>(target_bytes: usize) -> usize
where
    V: Semigroup + Clone,
    M: Clone,
{
    let branch_size = size_of::<SegNodeLayout<V, M, Ref, 0>>();
    let target = if target_bytes < branch_size {
        branch_size
    } else {
        target_bytes
    };

    capacity_for_linear_size(
        size_of::<SegBlockNodeLayout<V, M, Ref, 0>>(),
        size_of::<SegBlockNodeLayout<V, M, Ref, 1>>(),
        target,
    )
}

/// 线段树懒标记。
///
/// `apply` 会直接修改目标值，便于原址更新路径避免克隆整个 `V`。语义上要求：
/// 对任意 `a/b`，先合并后应用标记，等价于分别应用标记后再合并。
pub trait Applier<V: Semigroup> {
    fn apply(&self, to: &mut V);

    /// 非原址更新和查询下推仍需要得到一个新值；这里集中保留
    /// “克隆后原址应用”的默认实现。
    fn applied(&self, mut to: V) -> V {
        self.apply(&mut to);
        to
    }

    /// 判断这个懒标记是否可能改变某个节点代表的整个区间。
    ///
    /// 默认返回 true，表示不做剪枝。覆写时必须保证：返回 false 意味着把该懒标记
    /// 应用到这个节点覆盖的每一个元素上都不会改变任何元素；仅仅“聚合值不变”不够。
    /// 例如颜色集合 bitmask 可以判断“区间里完全没有会被替换的颜色”，但只保存右端
    /// 颜色的 assign-like monoid 不能安全剪枝，因为区间内部元素仍可能被改变。
    fn affects(&self, _value: &V) -> bool {
        true
    }

    /// 对叶块做批量应用。
    ///
    /// 默认逐个调用 `apply`。需要 SIMD、查表或其它数据并行优化时可以覆写这个方法；
    /// 这是执行优化，不改变 `Applier` 的代数语义。
    fn apply_slice(&self, to: &mut [V]) {
        for value in to {
            self.apply(value);
        }
    }
}

impl<A: Semigroup, B: Semigroup, MA: Applier<A>, MB: Applier<B>> Applier<(A, B)> for (MA, MB) {
    fn apply(&self, (a, b): &mut (A, B)) {
        let (ma, mb) = self;
        ma.apply(a);
        mb.apply(b);
    }

    fn affects(&self, (a, b): &(A, B)) -> bool {
        let (ma, mb) = self;
        ma.affects(a) || mb.affects(b)
    }
}

impl<A: Semigroup, M: Applier<A>> Applier<A> for Option<M> {
    fn apply(&self, to: &mut A) {
        if let Some(f) = self {
            f.apply(to);
        }
    }

    fn affects(&self, value: &A) -> bool {
        match self {
            Some(f) => f.affects(value),
            None => false,
        }
    }

    fn apply_slice(&self, to: &mut [A]) {
        if let Some(f) = self {
            f.apply_slice(to);
        }
    }
}

type SegRef<V, M, const B: usize, R> = <R as RefStore<SegNode<V, M, B, R>>>::Ref;
type StoreNode<V, M, const B: usize, F> = SegNode<V, M, B, SegTreeStore<V, M, B, F>>;
type InnerStore<V, M, const B: usize, F> = <F as RefStoreFactory>::Store<StoreNode<V, M, B, F>>;
type BaseArenaSegTreeStore<'base, V, M, const B: usize> =
    SegTreeStore<V, M, B, ArenaStoreFactory<'base>>;

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
    B,
    LayeredArenaStoreFactory<
        'store,
        'scratch,
        BaseArenaSegTreeStore<'base, V, M, B>,
        SegTreeLayerMapper<'store, 'base, 'scratch, V, M, B>,
    >,
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

pub struct SegTreeStore<V, M, const B: usize = DEFAULT_SEG_LEAF_BLOCK_CAPACITY, F = RcStoreFactory>
where
    V: Semigroup + Clone,
    M: Clone,
    F: RefStoreFactory,
    InnerStore<V, M, B, F>: RefStore<StoreNode<V, M, B, F>>,
{
    nodes: InnerStore<V, M, B, F>,
}

impl<V, M, const B: usize, F> SegTreeStore<V, M, B, F>
where
    V: Semigroup + Clone,
    M: Clone,
    F: RefStoreFactory,
    InnerStore<V, M, B, F>: RefStore<StoreNode<V, M, B, F>>,
{
    pub fn new(factory: F) -> Self {
        Self {
            nodes: factory.store(),
        }
    }
}

impl<V, M, const B: usize, F> Default for SegTreeStore<V, M, B, F>
where
    V: Semigroup + Clone,
    M: Clone,
    F: RefStoreFactory + Default,
    InnerStore<V, M, B, F>: RefStore<StoreNode<V, M, B, F>>,
{
    fn default() -> Self {
        Self::new(F::default())
    }
}

impl<'base, V, M, const B: usize> SegTreeStore<V, M, B, ArenaStoreFactory<'base>>
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
        tree: &SegTree<V, M, B, SegTreeStore<V, M, B, ArenaStoreFactory<'base>>>,
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
    type Source = SegNode<V, M, B, SegTreeStore<V, M, B, ArenaStoreFactory<'base>>>;
}

impl<'store, 'base, 'scratch, V, M, const B: usize>
    From<&SegNode<V, M, B, SegTreeStore<V, M, B, ArenaStoreFactory<'base>>>>
    for SegNode<V, M, B, LayeredArenaSegTreeStore<'store, 'base, 'scratch, V, M, B>>
where
    V: Semigroup + Clone,
    M: Clone,
{
    fn from(value: &SegNode<V, M, B, SegTreeStore<V, M, B, ArenaStoreFactory<'base>>>) -> Self {
        value.map_refs(&|reference| LayeredRef::Base(*reference))
    }
}

impl<V, M, const B: usize, S> RefStore<StoreNode<V, M, B, S>> for SegTreeStore<V, M, B, S>
where
    V: Semigroup + Clone,
    M: Clone,
    S: RefStoreFactory,
    InnerStore<V, M, B, S>: RefStore<StoreNode<V, M, B, S>>,
{
    type Ref = <InnerStore<V, M, B, S> as RefStore<StoreNode<V, M, B, S>>>::Ref;

    fn alloc(&mut self, value: StoreNode<V, M, B, S>) -> Self::Ref {
        self.nodes.alloc(value)
    }

    fn with_ref<T, C>(&self, reference: &Self::Ref, f: C) -> T
    where
        C: FnOnce(&StoreNode<V, M, B, S>) -> T,
    {
        self.nodes.with_ref(reference, f)
    }
}

impl<V, M, const B: usize, S> RefStoreMut<StoreNode<V, M, B, S>> for SegTreeStore<V, M, B, S>
where
    V: Semigroup + Clone,
    M: Clone,
    S: RefStoreFactory,
    InnerStore<V, M, B, S>: RefStoreMut<StoreNode<V, M, B, S>>,
{
    fn ref_mut(&mut self, reference: &Self::Ref) -> &mut SegNode<V, M, B, Self> {
        self.nodes.ref_mut(reference)
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
    R = SegTreeStore<V, M, B, RcStoreFactory>,
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
    R = SegTreeStore<V, M, B, RcStoreFactory>,
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
    R = SegTreeStore<V, M, B, RcStoreFactory>,
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
    fn range_bounds(range: Range<usize>) -> Range<usize> {
        min(range.start, N)..min(range.end, N)
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

    fn all(&self) -> V {
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
    fn apply_all_mut<M>(&mut self, m: &M)
    where
        M: Applier<V> + Monoid + Clone,
    {
        if m.is_empty() || !m.affects(&self.value) {
            return;
        }
        m.apply_slice(&mut self.items[..N]);
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
    fn query(&self, range: Range<usize>) -> V {
        if range.is_empty() {
            return V::empty();
        }
        if range.start == 0 && range.end >= N {
            return self.all();
        }
        self.items[Self::range_bounds(range)]
            .iter()
            .cloned()
            .reduce(Semigroup::merge)
            .unwrap_or_else(V::empty)
    }

    fn apply_mut<M>(&mut self, range: Range<usize>, m: &M)
    where
        M: Applier<V> + Monoid + Clone,
    {
        if m.is_empty() || range.is_empty() || !m.affects(&self.value) {
            return;
        }
        if range.start == 0 && range.end >= N {
            self.apply_all_mut(m);
            return;
        }

        m.apply_slice(&mut self.items[Self::range_bounds(range)]);
        self.value = self
            .items
            .iter()
            .cloned()
            .reduce(Semigroup::merge)
            .unwrap_or_else(V::empty);
    }
}

impl<V, M, const B: usize, R> SegTree<V, M, B, R>
where
    V: Monoid + Clone,
    M: Monoid + Clone,
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
    M: Monoid + Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
    pub fn build_in<F: Fn(usize) -> V + Clone>(refs: &mut R, len: usize, init: F) -> Self {
        assert!(B > 0, "seg tree leaf block capacity must be positive");
        let root = SegNode::build_inner(refs, 0, len, init);
        Self { len, root }
    }
}

impl<V, M, const B: usize, R> SegTree<V, M, B, R>
where
    V: Monoid + Clone,
    M: Applier<V> + Monoid + Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
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
}

impl<V, M, const B: usize, R> SegTree<V, M, B, R>
where
    V: Monoid + Clone,
    M: Applier<V> + Monoid + Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
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
    fn all(&self) -> V {
        match self {
            Self::Empty => V::empty(),
            Self::Block(block) => block.all(),
            Self::Branch { value, .. } => value.clone(),
        }
    }
}

impl<V, M, const B: usize, R> SegNode<V, M, B, R>
where
    V: Monoid + Clone,
    M: Applier<V> + Monoid + Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
    fn apply_all_mut(&mut self, m: &M) {
        if m.is_empty() {
            return;
        }
        match self {
            Self::Empty => {}
            Self::Block(block) => block.apply_all_mut(m),
            Self::Branch {
                modifier, value, ..
            } => {
                if !m.affects(value) {
                    return;
                }
                m.apply(value);
                modifier.prepend_assign(m);
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
            let value = left.all().merge(right.all());

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
            Self::Block(block) => block.query(range),
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
}

impl<V, M, const B: usize, R> SegNode<V, M, B, R>
where
    V: Monoid + Clone,
    M: Applier<V> + Monoid + Clone,
    R: RefStore<SegNode<V, M, B, R>>,
{
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
            Self::Block(block) => block.apply_mut(range, m),
            Self::Branch {
                left_size,
                modifier,
                value,
                left,
                right,
            } => {
                if !m.affects(value) {
                    return;
                }
                if range.start == 0 && size <= range.end {
                    m.apply(value);
                    modifier.prepend_assign(m);
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
                        left_node.apply_all_mut(modifier);
                    }
                    if let Some(range) = left_range {
                        left_node.apply_owned_mut(refs, mid, range, m);
                    }
                    let value = left_node.all();
                    (refs.alloc(left_node), value)
                } else {
                    (old_left, refs.with_ref(left, |node| node.all()))
                };

                let (right_ref, right_value) = if push_modifier || right_range.is_some() {
                    let mut right_node = refs.with_ref(&old_right, |node| node.clone());
                    if push_modifier {
                        right_node.apply_all_mut(modifier);
                    }
                    if let Some(range) = right_range {
                        right_node.apply_owned_mut(refs, right_size, range, m);
                    }
                    let value = right_node.all();
                    (refs.alloc(right_node), value)
                } else {
                    (old_right, refs.with_ref(right, |node| node.all()))
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
        if m.is_empty() || range.is_empty() {
            return;
        }
        match self {
            Self::Empty => {}
            Self::Block(block) => block.apply_mut(range, m),
            Self::Branch {
                left_size,
                modifier,
                value,
                left,
                right,
            } => {
                if !m.affects(value) {
                    return;
                }
                if range.start == 0 && size <= range.end {
                    m.apply(value);
                    modifier.prepend_assign(m);
                    return;
                }

                let mid = *left_size;
                let right_size = size - mid;
                let push_modifier = !modifier.is_empty();
                let left_hit = range.start < mid;
                let right_hit = mid < range.end;
                let left_ref = left.clone();
                let right_ref = right.clone();

                if push_modifier || (left_hit && right_hit) {
                    let mut left_node = std::mem::replace(refs.ref_mut(&left_ref), Self::Empty);
                    if push_modifier {
                        left_node.apply_all_mut(modifier);
                    }
                    if left_hit {
                        left_node.apply_mut(refs, mid, range.start..min(range.end, mid), m);
                    }
                    let left_value = left_node.all();
                    *refs.ref_mut(&left_ref) = left_node;

                    let mut right_node = std::mem::replace(refs.ref_mut(&right_ref), Self::Empty);
                    if push_modifier {
                        right_node.apply_all_mut(modifier);
                    }
                    if right_hit {
                        right_node.apply_mut(
                            refs,
                            right_size,
                            max(range.start, mid) - mid..range.end - mid,
                            m,
                        );
                    }
                    let right_value = right_node.all();
                    *refs.ref_mut(&right_ref) = right_node;

                    *modifier = M::empty();
                    *value = left_value.merge(right_value);
                } else if left_hit {
                    let mut left_node = std::mem::replace(refs.ref_mut(&left_ref), Self::Empty);
                    left_node.apply_mut(refs, mid, range.start..min(range.end, mid), m);
                    let left_value = left_node.all();
                    *refs.ref_mut(&left_ref) = left_node;

                    let right_value = refs.with_ref(&right_ref, |node| node.all());
                    *value = left_value.merge(right_value);
                } else if right_hit {
                    let mut right_node = std::mem::replace(refs.ref_mut(&right_ref), Self::Empty);
                    right_node.apply_mut(
                        refs,
                        right_size,
                        max(range.start, mid) - mid..range.end - mid,
                        m,
                    );
                    let right_value = right_node.all();
                    *refs.ref_mut(&right_ref) = right_node;

                    let left_value = refs.with_ref(&left_ref, |node| node.all());
                    *value = left_value.merge(right_value);
                }
            }
        }
    }
}
