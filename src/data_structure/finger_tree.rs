use crate::data_structure::ref_store::{
    ArcStoreFactory, ArenaStoreFactory, ConstArenaStoreFactory, LayeredArenaStoreFactory,
    LayeredRef, RcStoreFactory, RefMapper, RefStore, RefStoreFactory,
};
use crate::traits::{monoid::Monoid, monoid::Size, semigroup::Semigroup};
use std::{
    marker::PhantomData,
    mem::{size_of, ManuallyDrop, MaybeUninit},
    slice,
};

pub mod prelude {
    pub use super::{
        chunk_capacity_for_bytes, Chunk, FingerTree, FingerTreeStore, Measured, Value,
        CACHE_LINE_BYTES,
    };
}

pub trait Measured: Clone {
    type Measure: Monoid + Clone;

    fn measure(&self) -> Self::Measure;
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Value<T>(pub T);

impl<T: Clone> Measured for Value<T> {
    type Measure = Size;
    fn measure(&self) -> Self::Measure {
        Size(1)
    }
}

/// 常见 CPU cache line 的字节数。竞赛环境里通常不值得为不同机器继续细分。
pub const CACHE_LINE_BYTES: usize = 64;

/// 按目标字节数估算 `Chunk<A, N>` 的容量。
///
/// 这里故意只返回一个普通 `usize` 常量：stable Rust 目前还不能把依赖泛型参数的
/// `size_of::<A>()` 结果直接塞进泛型类型的数组长度里。因此底层仍然保留
/// `Chunk<A, const N>`，调用侧可以用这个函数生成 `N`，避免手写魔法数字。
///
/// 实现上使用 `Chunk<A, 0>` 和 `Chunk<A, 1>` 的真实布局差来估算每个元素带来的
/// 增量，而不是手写 `measure + len + items` 的字段大小。这样如果 `Chunk` 的字段布局、
/// 对齐或尾部填充发生变化，容量估算会跟着实际类型布局一起变化。
pub const fn chunk_capacity_for_bytes<A: Measured>(target_bytes: usize) -> usize {
    let zero = size_of::<Chunk<A, 0>>();
    let one = size_of::<Chunk<A, 1>>();
    let item = one.saturating_sub(zero);

    match target_bytes.saturating_sub(zero).checked_div(item) {
        Some(cap) if cap > 0 => cap,
        _ => 1,
    }
}

pub struct Chunk<A: Measured, const N: usize> {
    measure: A::Measure,
    len: usize,
    items: [MaybeUninit<A>; N],
}

pub struct FingerTree<
    A: Measured,
    const N: usize = 1,
    R: FingerTreeRefs<Chunk<A, N>> = FingerTreeStore<Chunk<A, N>>,
> {
    chunks: RawFingerTree<Chunk<A, N>, R>,
}

impl<A: Measured, const N: usize> Chunk<A, N> {
    fn empty() -> Self {
        assert!(N > 0, "chunk capacity must be positive");
        Self {
            measure: A::Measure::empty(),
            len: 0,
            items: std::array::from_fn(|_| MaybeUninit::uninit()),
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn is_full(&self) -> bool {
        self.len == N
    }

    fn as_slice(&self) -> &[A] {
        // SAFETY: `push_back` 只会初始化 `[0, len)`，并且 `len` 从不超过 N。
        unsafe { slice::from_raw_parts(self.items.as_ptr().cast::<A>(), self.len) }
    }

    fn push_back(&mut self, value: A) {
        assert!(self.len < N, "chunk capacity exceeded");
        append_measure(&mut self.measure, value.measure());
        self.items[self.len].write(value);
        self.len += 1;
    }

    fn push_front(&mut self, value: A) {
        assert!(self.len < N, "chunk capacity exceeded");
        for i in (0..self.len).rev() {
            // SAFETY: `[0, len)` 已初始化，向右平移一格，目标槽位不会被读取。
            unsafe {
                let item = self.items[i].as_ptr().read();
                self.items[i + 1].write(item);
            }
        }
        prepend_measure(&mut self.measure, value.measure());
        self.items[0].write(value);
        self.len += 1;
    }

    fn singleton(value: A) -> Self {
        let mut chunk = Self::empty();
        chunk.push_back(value);
        chunk
    }

    fn pop_front(&mut self) -> Option<A> {
        if self.len == 0 {
            return None;
        }
        // SAFETY: `len > 0`，首槽已初始化；后续槽位左移覆盖空洞。
        let value = unsafe { self.items[0].as_ptr().read() };
        for i in 1..self.len {
            // SAFETY: `i < len`，源槽位已初始化；目标槽位之前已被移出或覆盖。
            unsafe {
                let item = self.items[i].as_ptr().read();
                self.items[i - 1].write(item);
            }
        }
        self.len -= 1;
        self.recompute_measure();
        Some(value)
    }

    fn pop_back(&mut self) -> Option<A> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: 递减后的 `len` 正是旧最后一个已初始化槽位。
        let value = unsafe { self.items[self.len].as_ptr().read() };
        self.recompute_measure();
        Some(value)
    }

    fn recompute_measure(&mut self) {
        self.measure = self
            .as_slice()
            .iter()
            .map(|value| value.measure())
            .reduce(Semigroup::merge)
            .unwrap_or_else(A::Measure::empty);
    }

    fn first(&self) -> Option<A> {
        self.as_slice().first().cloned()
    }

    fn last(&self) -> Option<A> {
        self.as_slice().last().cloned()
    }

    fn split_offset<F>(
        self,
        mut offset: A::Measure,
        pred: &F,
    ) -> Option<(Option<Self>, A, Option<Self>)>
    where
        F: Fn(&A::Measure) -> bool,
    {
        let mut split_index = None;
        for (i, value) in self.as_slice().iter().enumerate() {
            let next = offset.clone().merge(value.measure());
            if pred(&next) {
                split_index = Some(i);
                break;
            } else {
                offset = next;
            }
        }

        let split_index = split_index?;
        let this = ManuallyDrop::new(self);
        let before = (split_index > 0).then(|| {
            let mut chunk = Self::empty();
            for i in 0..split_index {
                // SAFETY: `i < split_index <= len`，对应槽位已初始化；`this`
                // 被 `ManuallyDrop` 持有，后面不会再自动析构这些已搬走的值。
                chunk.push_back(unsafe { this.items[i].as_ptr().read() });
            }
            chunk
        });

        // SAFETY: `split_index` 来自 `[0, len)` 的扫描结果，该槽位已初始化。
        let middle = unsafe { this.items[split_index].as_ptr().read() };

        let after = (split_index + 1 < this.len).then(|| {
            let mut chunk = Self::empty();
            for i in split_index + 1..this.len {
                // SAFETY: 同上，右半边槽位均在原 `len` 范围内且已初始化。
                chunk.push_back(unsafe { this.items[i].as_ptr().read() });
            }
            chunk
        });

        Some((before, middle, after))
    }
}

impl<A: Measured, const N: usize> Clone for Chunk<A, N> {
    fn clone(&self) -> Self {
        let mut chunk = Self::empty();
        for value in self.as_slice() {
            chunk.push_back(value.clone());
        }
        chunk
    }
}

impl<A: Measured, const N: usize> Drop for Chunk<A, N> {
    fn drop(&mut self) {
        for item in &mut self.items[..self.len] {
            // SAFETY: `[0, len)` 内的元素均由 `push_back` 初始化，且只在这里析构一次。
            unsafe {
                item.assume_init_drop();
            }
        }
    }
}

impl<A: Measured, const N: usize> Measured for Chunk<A, N> {
    type Measure = A::Measure;

    fn measure(&self) -> Self::Measure {
        self.measure.clone()
    }
}

impl<A, const N: usize, R> Clone for FingerTree<A, N, R>
where
    A: Measured,
    R: FingerTreeRefs<Chunk<A, N>>,
{
    fn clone(&self) -> Self {
        Self {
            chunks: self.chunks.clone(),
        }
    }
}

impl<A, const N: usize, R> FingerTree<A, N, R>
where
    A: Measured,
    R: FingerTreeRefs<Chunk<A, N>>,
{
    #[inline]
    pub fn new() -> Self {
        Self {
            chunks: RawFingerTree::new(),
        }
    }

    #[inline]
    pub fn from_iter_in<T>(refs: &mut R, iter: T) -> Self
    where
        T: IntoIterator<Item = A>,
    {
        let mut chunks = RawFingerTree::new();
        let mut chunk = Chunk::empty();
        for value in iter {
            if chunk.is_full() {
                chunks.push_back_mut(refs, chunk);
                chunk = Chunk::empty();
            }
            chunk.push_back(value);
        }
        if !chunk.is_empty() {
            chunks.push_back_mut(refs, chunk);
        }
        Self { chunks }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    #[inline]
    pub fn measure(&self) -> A::Measure {
        self.chunks.measure()
    }

    #[inline]
    pub fn front(&self, refs: &R) -> Option<A> {
        self.chunks.front(refs).and_then(|chunk| chunk.first())
    }

    #[inline]
    pub fn back(&self, refs: &R) -> Option<A> {
        self.chunks.back(refs).and_then(|chunk| chunk.last())
    }

    #[inline]
    pub fn concat(&self, refs: &mut R, other: &Self) -> Self {
        Self {
            chunks: self.chunks.concat(refs, &other.chunks),
        }
    }

    #[inline]
    pub fn concat_mut(&mut self, refs: &mut R, other: Self) {
        self.chunks.concat_mut(refs, other.chunks);
    }

    #[inline]
    pub fn into_concat(mut self, refs: &mut R, other: Self) -> Self {
        self.concat_mut(refs, other);
        self
    }

    #[inline]
    pub fn push_front(&self, refs: &mut R, value: A) -> Self {
        self.clone().into_push_front(refs, value)
    }

    #[inline]
    pub fn push_back(&self, refs: &mut R, value: A) -> Self {
        self.clone().into_push_back(refs, value)
    }

    #[inline]
    pub fn push_front_mut(&mut self, refs: &mut R, value: A) {
        match self.chunks.pop_front(refs) {
            Some(mut chunk) if !chunk.is_full() => {
                chunk.push_front(value);
                self.chunks.push_front_mut(refs, chunk);
            }
            Some(chunk) => {
                self.chunks.push_front_mut(refs, chunk);
                self.chunks.push_front_mut(refs, Chunk::singleton(value));
            }
            None => self.chunks.push_front_mut(refs, Chunk::singleton(value)),
        }
    }

    #[inline]
    pub fn push_back_mut(&mut self, refs: &mut R, value: A) {
        match self.chunks.pop_back(refs) {
            Some(mut chunk) if !chunk.is_full() => {
                chunk.push_back(value);
                self.chunks.push_back_mut(refs, chunk);
            }
            Some(chunk) => {
                self.chunks.push_back_mut(refs, chunk);
                self.chunks.push_back_mut(refs, Chunk::singleton(value));
            }
            None => self.chunks.push_back_mut(refs, Chunk::singleton(value)),
        }
    }

    #[inline]
    pub fn into_push_front(mut self, refs: &mut R, value: A) -> Self {
        self.push_front_mut(refs, value);
        self
    }

    #[inline]
    pub fn into_push_back(mut self, refs: &mut R, value: A) -> Self {
        self.push_back_mut(refs, value);
        self
    }

    #[inline]
    pub fn pop_front(&mut self, refs: &mut R) -> Option<A> {
        let mut chunk = self.chunks.pop_front(refs)?;
        let value = chunk.pop_front()?;
        if !chunk.is_empty() {
            self.chunks.push_front_mut(refs, chunk);
        }
        Some(value)
    }

    #[inline]
    pub fn pop_back(&mut self, refs: &mut R) -> Option<A> {
        let mut chunk = self.chunks.pop_back(refs)?;
        let value = chunk.pop_back()?;
        if !chunk.is_empty() {
            self.chunks.push_back_mut(refs, chunk);
        }
        Some(value)
    }

    #[inline]
    pub fn view_front(&self, refs: &mut R) -> Option<(A, Self)> {
        let mut tree = self.clone();
        tree.pop_front(refs).map(|value| (value, tree))
    }

    #[inline]
    pub fn view_back(&self, refs: &mut R) -> Option<(Self, A)> {
        let mut tree = self.clone();
        tree.pop_back(refs).map(|value| (tree, value))
    }

    #[inline]
    pub fn into_view_front(mut self, refs: &mut R) -> Option<(A, Self)> {
        self.pop_front(refs).map(|value| (value, self))
    }

    #[inline]
    pub fn into_view_back(mut self, refs: &mut R) -> Option<(Self, A)> {
        self.pop_back(refs).map(|value| (self, value))
    }

    #[inline]
    pub fn split<F>(&self, refs: &mut R, pred: F) -> Option<(Self, A, Self)>
    where
        F: Fn(&A::Measure) -> bool,
    {
        let (mut front, chunk, mut back) = self.chunks.split(refs, |measure| pred(measure))?;
        let offset = front.measure();
        let (before, value, after) = chunk.split_offset(offset, &pred)?;
        if let Some(before) = before {
            front.push_back_mut(refs, before);
        }
        if let Some(after) = after {
            back.push_front_mut(refs, after);
        }
        Some((Self { chunks: front }, value, Self { chunks: back }))
    }

    #[inline]
    pub fn into_split<F>(self, refs: &mut R, pred: F) -> Option<(Self, A, Self)>
    where
        F: Fn(&A::Measure) -> bool,
    {
        let (mut front, chunk, mut back) = self.chunks.into_split(refs, |measure| pred(measure))?;
        let offset = front.measure();
        let (before, value, after) = chunk.split_offset(offset, &pred)?;
        if let Some(before) = before {
            front.push_back_mut(refs, before);
        }
        if let Some(after) = after {
            back.push_front_mut(refs, after);
        }
        Some((Self { chunks: front }, value, Self { chunks: back }))
    }

    #[inline]
    fn map_refs<S, FN, FT>(&self, node_map: &FN, tree_map: &FT) -> FingerTree<A, N, S>
    where
        S: FingerTreeRefs<Chunk<A, N>>,
        FN: Fn(&NodeRef<Chunk<A, N>, R>) -> NodeRef<Chunk<A, N>, S>,
        FT: Fn(&TreeRef<Chunk<A, N>, R>) -> TreeRef<Chunk<A, N>, S>,
    {
        FingerTree {
            chunks: self.chunks.map_refs(node_map, tree_map),
        }
    }
}

impl<A, const N: usize, R> Default for FingerTree<A, N, R>
where
    A: Measured,
    R: FingerTreeRefs<Chunk<A, N>>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<A, const N: usize, R> Measured for FingerTree<A, N, R>
where
    A: Measured,
    R: FingerTreeRefs<Chunk<A, N>>,
{
    type Measure = A::Measure;
    fn measure(&self) -> Self::Measure {
        self.chunks.measure()
    }
}

impl<A> FromIterator<A> for FingerTree<A>
where
    A: Measured,
{
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let mut refs = FingerTreeStore::default();
        Self::from_iter_in(&mut refs, iter)
    }
}

pub trait FingerTreeRefs<A: Measured>:
    Sized + RefStore<Node<A, Self>> + RefStore<Tree<A, Self>>
{
    fn alloc_node(&mut self, node: Node<A, Self>) -> NodeRef<A, Self> {
        <Self as RefStore<Node<A, Self>>>::alloc(self, node)
    }

    fn alloc_tree(&mut self, tree: Tree<A, Self>) -> TreeRef<A, Self> {
        <Self as RefStore<Tree<A, Self>>>::alloc(self, tree)
    }

    fn with_node<T, F>(&self, node: &NodeRef<A, Self>, f: F) -> T
    where
        F: FnOnce(&Node<A, Self>) -> T,
    {
        <Self as RefStore<Node<A, Self>>>::with_ref(self, node, f)
    }

    fn with_tree<T, F>(&self, tree: &TreeRef<A, Self>, f: F) -> T
    where
        F: FnOnce(&Tree<A, Self>) -> T,
    {
        <Self as RefStore<Tree<A, Self>>>::with_ref(self, tree, f)
    }

    fn measure_node_ref(&self, node: &NodeRef<A, Self>) -> A::Measure {
        self.with_node(node, |node| node.measure())
    }

    fn measure_tree_ref(&self, tree: &TreeRef<A, Self>) -> A::Measure {
        self.with_tree(tree, |tree| tree.measure())
    }

    fn clone_tree_ref(&self, tree: &TreeRef<A, Self>) -> Tree<A, Self> {
        self.with_tree(tree, Clone::clone)
    }

    fn leaf_value(&self, node: &NodeRef<A, Self>) -> A {
        self.with_node(node, |node| match &node.inner {
            NodeInner::Leaf(value) => value.clone(),
            NodeInner::Branch2 { .. } | NodeInner::Branch3 { .. } => {
                // 对外的 view/split 只会在根层调用；论文里这一层的逻辑元素
                // 类型是 `A`。
                unreachable!("top-level tree operation returned an internal branch")
            }
        })
    }

    fn node_to_digit(&self, node: &NodeRef<A, Self>) -> Digit<NodeRef<A, Self>> {
        self.with_node(node, |node| match &node.inner {
            NodeInner::Branch2 { left, right } => Digit::Two([left.clone(), right.clone()]),
            NodeInner::Branch3 {
                left,
                middle,
                right,
            } => Digit::Three([left.clone(), middle.clone(), right.clone()]),
            // 只有来自中间树的递归结果会被展开成 Digit。论文里这些树的元素类型是
            // `Node v a`，不可能是 `a`。
            NodeInner::Leaf(_) => unreachable!("leaf node cannot be unlifted"),
        })
    }
}

type FingerStoreNode<A, F> = Node<A, FingerTreeStore<A, F>>;
type FingerStoreTree<A, F> = Tree<A, FingerTreeStore<A, F>>;
type FingerNodeStore<A, F> = <F as RefStoreFactory>::Store<FingerStoreNode<A, F>>;
type FingerTreeStoreInner<A, F> = <F as RefStoreFactory>::Store<FingerStoreTree<A, F>>;
type BaseArenaFingerTreeStore<'base, A> = FingerTreeStore<A, ArenaStoreFactory<'base>>;

pub type LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A> = FingerTreeStore<
    A,
    LayeredArenaStoreFactory<
        'store,
        'scratch,
        BaseArenaFingerTreeStore<'base, A>,
        FingerTreeLayerMapper<'store, 'base, 'scratch, A>,
    >,
>;

type FingerTreeLayerMarker<'store, 'base, 'scratch, A> =
    PhantomData<fn() -> (&'store (), &'base (), &'scratch (), A)>;

#[doc(hidden)]
pub struct FingerTreeLayerMapper<'store, 'base, 'scratch, A: Measured>(
    FingerTreeLayerMarker<'store, 'base, 'scratch, A>,
);

pub trait FingerTreeStoreFactory<A: Measured>: RefStoreFactory + Sized {
    #[inline]
    fn measure_node_ref(
        store: &FingerTreeStore<A, Self>,
        node: &NodeRef<A, FingerTreeStore<A, Self>>,
    ) -> A::Measure
    where
        FingerNodeStore<A, Self>: RefStore<FingerStoreNode<A, Self>>,
        FingerTreeStoreInner<A, Self>: RefStore<FingerStoreTree<A, Self>>,
        FingerTreeStore<A, Self>: RefStore<Node<A, FingerTreeStore<A, Self>>>,
    {
        <FingerTreeStore<A, Self> as RefStore<Node<A, FingerTreeStore<A, Self>>>>::with_ref(
            store,
            node,
            |node| node.measure(),
        )
    }

    #[inline]
    fn measure_tree_ref(
        store: &FingerTreeStore<A, Self>,
        tree: &TreeRef<A, FingerTreeStore<A, Self>>,
    ) -> A::Measure
    where
        FingerNodeStore<A, Self>: RefStore<FingerStoreNode<A, Self>>,
        FingerTreeStoreInner<A, Self>: RefStore<FingerStoreTree<A, Self>>,
        FingerTreeStore<A, Self>: RefStore<Tree<A, FingerTreeStore<A, Self>>>,
    {
        <FingerTreeStore<A, Self> as RefStore<Tree<A, FingerTreeStore<A, Self>>>>::with_ref(
            store,
            tree,
            |tree| tree.measure(),
        )
    }

    #[inline]
    fn clone_tree_ref(
        store: &FingerTreeStore<A, Self>,
        tree: &TreeRef<A, FingerTreeStore<A, Self>>,
    ) -> Tree<A, FingerTreeStore<A, Self>>
    where
        FingerNodeStore<A, Self>: RefStore<FingerStoreNode<A, Self>>,
        FingerTreeStoreInner<A, Self>: RefStore<FingerStoreTree<A, Self>>,
        FingerTreeStore<A, Self>: RefStore<Tree<A, FingerTreeStore<A, Self>>>,
    {
        <FingerTreeStore<A, Self> as RefStore<Tree<A, FingerTreeStore<A, Self>>>>::with_ref(
            store,
            tree,
            Clone::clone,
        )
    }

    #[inline]
    fn leaf_value(
        store: &FingerTreeStore<A, Self>,
        node: &NodeRef<A, FingerTreeStore<A, Self>>,
    ) -> A
    where
        FingerNodeStore<A, Self>: RefStore<FingerStoreNode<A, Self>>,
        FingerTreeStoreInner<A, Self>: RefStore<FingerStoreTree<A, Self>>,
        FingerTreeStore<A, Self>: RefStore<Node<A, FingerTreeStore<A, Self>>>,
    {
        <FingerTreeStore<A, Self> as RefStore<Node<A, FingerTreeStore<A, Self>>>>::with_ref(
            store,
            node,
            |node| match &node.inner {
                NodeInner::Leaf(value) => value.clone(),
                NodeInner::Branch2 { .. } | NodeInner::Branch3 { .. } => {
                    unreachable!("top-level tree operation returned an internal branch")
                }
            },
        )
    }

    #[inline]
    fn node_to_digit(
        store: &FingerTreeStore<A, Self>,
        node: &NodeRef<A, FingerTreeStore<A, Self>>,
    ) -> Digit<NodeRef<A, FingerTreeStore<A, Self>>>
    where
        FingerNodeStore<A, Self>: RefStore<FingerStoreNode<A, Self>>,
        FingerTreeStoreInner<A, Self>: RefStore<FingerStoreTree<A, Self>>,
        FingerTreeStore<A, Self>: RefStore<Node<A, FingerTreeStore<A, Self>>>,
    {
        <FingerTreeStore<A, Self> as RefStore<Node<A, FingerTreeStore<A, Self>>>>::with_ref(
            store,
            node,
            |node| match &node.inner {
                NodeInner::Branch2 { left, right } => Digit::Two([left.clone(), right.clone()]),
                NodeInner::Branch3 {
                    left,
                    middle,
                    right,
                } => Digit::Three([left.clone(), middle.clone(), right.clone()]),
                NodeInner::Leaf(_) => unreachable!("leaf node cannot be unlifted"),
            },
        )
    }
}

impl<A: Measured> FingerTreeStoreFactory<A> for RcStoreFactory {}
impl<A: Measured> FingerTreeStoreFactory<A> for ArcStoreFactory {}
impl<'id, A: Measured> FingerTreeStoreFactory<A> for ArenaStoreFactory<'id> {}
impl<'id, const N: usize, A: Measured> FingerTreeStoreFactory<A>
    for ConstArenaStoreFactory<'id, N>
{
}

pub struct FingerTreeStore<A: Measured, F: FingerTreeStoreFactory<A> = RcStoreFactory>
where
    FingerNodeStore<A, F>: RefStore<FingerStoreNode<A, F>>,
    FingerTreeStoreInner<A, F>: RefStore<FingerStoreTree<A, F>>,
{
    nodes: FingerNodeStore<A, F>,
    trees: FingerTreeStoreInner<A, F>,
}

// `ArenaRef` 本身只是 arena 内的下标，所以普通 arena store 要求树和 store
// 来自同一个 region：否则同一个下标可能被拿去读另一块 arena。
//
// layered store 显式记录引用来源，读旧节点时走 base arena，分配新节点时走
// scratch arena。它的返回树带有 `'scratch`，因此类型系统会保证这些新引用先于
// 外层 base 释放；同时 `LayeredRef` 的 Base/Scratch 分支避免了下标串门。

impl<A, F> FingerTreeStore<A, F>
where
    A: Measured,
    F: FingerTreeStoreFactory<A>,
    FingerNodeStore<A, F>: RefStore<FingerStoreNode<A, F>>,
    FingerTreeStoreInner<A, F>: RefStore<FingerStoreTree<A, F>>,
{
    #[inline]
    pub fn new(factory: F) -> Self {
        Self {
            nodes: factory.store(),
            trees: factory.store(),
        }
    }
}

impl<A, F> Default for FingerTreeStore<A, F>
where
    A: Measured,
    F: FingerTreeStoreFactory<A> + Default,
    FingerNodeStore<A, F>: RefStore<FingerStoreNode<A, F>>,
    FingerTreeStoreInner<A, F>: RefStore<FingerStoreTree<A, F>>,
{
    fn default() -> Self {
        Self::new(F::default())
    }
}

impl<'base, A: Measured> FingerTreeStore<A, ArenaStoreFactory<'base>> {
    #[inline]
    pub fn layered<'store, T, F>(&'store self, capacity: usize, f: F) -> T
    where
        F: for<'scratch> FnOnce(LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>) -> T,
    {
        let factory = LayeredArenaStoreFactory::new(self, capacity);
        f(FingerTreeStore {
            nodes: factory.store_with_capacity(capacity),
            trees: factory.store_with_capacity(capacity / 4 + 1),
        })
    }
}

impl<'store, 'base, 'scratch, A: Measured> LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A> {
    #[inline]
    fn map_base_node(
        node: &Node<A, BaseArenaFingerTreeStore<'base, A>>,
    ) -> Node<A, LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>> {
        node.map_refs(&|reference| LayeredRef::Base(*reference))
    }

    #[inline]
    fn map_base_tree(
        tree: &Tree<A, BaseArenaFingerTreeStore<'base, A>>,
    ) -> Tree<A, LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>> {
        tree.map_refs(&|reference| LayeredRef::Base(*reference), &|reference| {
            LayeredRef::Base(*reference)
        })
    }
}

impl<'store, 'base, 'scratch, A: Measured, const N: usize>
    LayeredArenaFingerTreeStore<'store, 'base, 'scratch, Chunk<A, N>>
{
    #[inline]
    pub fn from_base(
        &self,
        tree: &FingerTree<A, N, BaseArenaFingerTreeStore<'base, Chunk<A, N>>>,
    ) -> FingerTree<A, N, Self> {
        tree.map_refs(&|reference| LayeredRef::Base(*reference), &|reference| {
            LayeredRef::Base(*reference)
        })
    }
}

impl<A, F> FingerTreeRefs<A> for FingerTreeStore<A, F>
where
    A: Measured,
    F: FingerTreeStoreFactory<A>,
    FingerNodeStore<A, F>: RefStore<FingerStoreNode<A, F>>,
    FingerTreeStoreInner<A, F>: RefStore<FingerStoreTree<A, F>>,
    Self: RefStore<Node<A, Self>> + RefStore<Tree<A, Self>>,
{
    #[inline]
    fn measure_node_ref(&self, node: &NodeRef<A, Self>) -> A::Measure {
        F::measure_node_ref(self, node)
    }

    #[inline]
    fn measure_tree_ref(&self, tree: &TreeRef<A, Self>) -> A::Measure {
        F::measure_tree_ref(self, tree)
    }

    #[inline]
    fn clone_tree_ref(&self, tree: &TreeRef<A, Self>) -> Tree<A, Self> {
        F::clone_tree_ref(self, tree)
    }

    #[inline]
    fn leaf_value(&self, node: &NodeRef<A, Self>) -> A {
        F::leaf_value(self, node)
    }

    #[inline]
    fn node_to_digit(&self, node: &NodeRef<A, Self>) -> Digit<NodeRef<A, Self>> {
        F::node_to_digit(self, node)
    }
}

impl<'store, 'base, 'scratch, A: Measured>
    RefMapper<Node<A, LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>>>
    for FingerTreeLayerMapper<'store, 'base, 'scratch, A>
{
    type Source = Node<A, BaseArenaFingerTreeStore<'base, A>>;
}

impl<'store, 'base, 'scratch, A: Measured> From<&Node<A, BaseArenaFingerTreeStore<'base, A>>>
    for Node<A, LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>>
{
    fn from(value: &Node<A, BaseArenaFingerTreeStore<'base, A>>) -> Self {
        FingerTreeStore::map_base_node(value)
    }
}

impl<'store, 'base, 'scratch, A: Measured>
    RefMapper<Tree<A, LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>>>
    for FingerTreeLayerMapper<'store, 'base, 'scratch, A>
{
    type Source = Tree<A, BaseArenaFingerTreeStore<'base, A>>;
}

impl<'store, 'base, 'scratch, A: Measured> From<&Tree<A, BaseArenaFingerTreeStore<'base, A>>>
    for Tree<A, LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>>
{
    fn from(value: &Tree<A, BaseArenaFingerTreeStore<'base, A>>) -> Self {
        FingerTreeStore::map_base_tree(value)
    }
}

impl<'store, 'base, 'scratch, A: Measured> FingerTreeStoreFactory<A>
    for LayeredArenaStoreFactory<
        'store,
        'scratch,
        BaseArenaFingerTreeStore<'base, A>,
        FingerTreeLayerMapper<'store, 'base, 'scratch, A>,
    >
{
    #[inline]
    fn measure_node_ref(
        store: &LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>,
        node: &NodeRef<A, LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>>,
    ) -> A::Measure {
        match node {
            LayeredRef::Base(node) => store.nodes.with_base_ref(
                node,
                |node: &Node<A, BaseArenaFingerTreeStore<'base, A>>| node.measure(),
            ),
            LayeredRef::Scratch(node) => store.nodes.with_scratch_ref(node, |node| node.measure()),
        }
    }

    #[inline]
    fn measure_tree_ref(
        store: &LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>,
        tree: &TreeRef<A, LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>>,
    ) -> A::Measure {
        match tree {
            LayeredRef::Base(tree) => store.trees.with_base_ref(
                tree,
                |tree: &Tree<A, BaseArenaFingerTreeStore<'base, A>>| tree.measure(),
            ),
            LayeredRef::Scratch(tree) => store.trees.with_scratch_ref(tree, |tree| tree.measure()),
        }
    }

    #[inline]
    fn clone_tree_ref(
        store: &LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>,
        tree: &TreeRef<A, LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>>,
    ) -> Tree<A, LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>> {
        match tree {
            LayeredRef::Base(tree) => store
                .trees
                .with_base_ref(tree, FingerTreeStore::map_base_tree),
            LayeredRef::Scratch(tree) => store.trees.with_scratch_ref(tree, Clone::clone),
        }
    }

    #[inline]
    fn leaf_value(
        store: &LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>,
        node: &NodeRef<A, LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>>,
    ) -> A {
        match node {
            LayeredRef::Base(node) => store.nodes.with_base_ref(
                node,
                |node: &Node<A, BaseArenaFingerTreeStore<'base, A>>| match &node.inner {
                    NodeInner::Leaf(value) => value.clone(),
                    NodeInner::Branch2 { .. } | NodeInner::Branch3 { .. } => {
                        unreachable!("top-level tree operation returned an internal branch")
                    }
                },
            ),
            LayeredRef::Scratch(node) => {
                store
                    .nodes
                    .with_scratch_ref(node, |node| match &node.inner {
                        NodeInner::Leaf(value) => value.clone(),
                        NodeInner::Branch2 { .. } | NodeInner::Branch3 { .. } => {
                            unreachable!("top-level tree operation returned an internal branch")
                        }
                    })
            }
        }
    }

    #[inline]
    fn node_to_digit(
        store: &LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>,
        node: &NodeRef<A, LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>>,
    ) -> Digit<NodeRef<A, LayeredArenaFingerTreeStore<'store, 'base, 'scratch, A>>> {
        match node {
            LayeredRef::Base(node) => store.nodes.with_base_ref(
                node,
                |node: &Node<A, BaseArenaFingerTreeStore<'base, A>>| match &node.inner {
                    NodeInner::Branch2 { left, right } => {
                        Digit::Two([LayeredRef::Base(*left), LayeredRef::Base(*right)])
                    }
                    NodeInner::Branch3 {
                        left,
                        middle,
                        right,
                    } => Digit::Three([
                        LayeredRef::Base(*left),
                        LayeredRef::Base(*middle),
                        LayeredRef::Base(*right),
                    ]),
                    NodeInner::Leaf(_) => unreachable!("leaf node cannot be unlifted"),
                },
            ),
            LayeredRef::Scratch(node) => {
                store
                    .nodes
                    .with_scratch_ref(node, |node| match &node.inner {
                        NodeInner::Branch2 { left, right } => Digit::Two([*left, *right]),
                        NodeInner::Branch3 {
                            left,
                            middle,
                            right,
                        } => Digit::Three([*left, *middle, *right]),
                        NodeInner::Leaf(_) => unreachable!("leaf node cannot be unlifted"),
                    })
            }
        }
    }
}

impl<A, F> RefStore<Node<A, FingerTreeStore<A, F>>> for FingerTreeStore<A, F>
where
    A: Measured,
    F: FingerTreeStoreFactory<A>,
    FingerNodeStore<A, F>: RefStore<Node<A, FingerTreeStore<A, F>>>,
    FingerTreeStoreInner<A, F>: RefStore<Tree<A, FingerTreeStore<A, F>>>,
{
    type Ref = <FingerNodeStore<A, F> as RefStore<Node<A, FingerTreeStore<A, F>>>>::Ref;

    #[inline]
    fn alloc(&mut self, value: Node<A, FingerTreeStore<A, F>>) -> Self::Ref {
        self.nodes.alloc(value)
    }

    #[inline]
    fn with_ref<T, C>(&self, reference: &Self::Ref, f: C) -> T
    where
        C: FnOnce(&Node<A, FingerTreeStore<A, F>>) -> T,
    {
        self.nodes.with_ref(reference, f)
    }
}

impl<A, F> RefStore<Tree<A, FingerTreeStore<A, F>>> for FingerTreeStore<A, F>
where
    A: Measured,
    F: FingerTreeStoreFactory<A>,
    FingerNodeStore<A, F>: RefStore<Node<A, FingerTreeStore<A, F>>>,
    FingerTreeStoreInner<A, F>: RefStore<Tree<A, FingerTreeStore<A, F>>>,
{
    type Ref = <FingerTreeStoreInner<A, F> as RefStore<Tree<A, FingerTreeStore<A, F>>>>::Ref;

    #[inline]
    fn alloc(&mut self, value: Tree<A, FingerTreeStore<A, F>>) -> Self::Ref {
        self.trees.alloc(value)
    }

    #[inline]
    fn with_ref<T, C>(&self, reference: &Self::Ref, f: C) -> T
    where
        C: FnOnce(&Tree<A, FingerTreeStore<A, F>>) -> T,
    {
        self.trees.with_ref(reference, f)
    }
}

type NodeRef<A, R> = <R as RefStore<Node<A, R>>>::Ref;
type TreeRef<A, R> = <R as RefStore<Tree<A, R>>>::Ref;
type DigitSplit<A, R> = (
    Option<Digit<NodeRef<A, R>>>,
    NodeRef<A, R>,
    Option<Digit<NodeRef<A, R>>>,
);

struct RawFingerTree<A: Measured, R: FingerTreeRefs<A> = FingerTreeStore<A>> {
    root: Tree<A, R>,
}

pub struct Tree<A: Measured, R: FingerTreeRefs<A>>(TreeInner<A, R>);

enum TreeInner<A: Measured, R: FingerTreeRefs<A>> {
    Empty,
    Single {
        measure: A::Measure,
        node: NodeRef<A, R>,
    },
    Deep {
        measure: A::Measure,
        prefix: Digit<NodeRef<A, R>>,
        deeper: TreeRef<A, R>,
        suffix: Digit<NodeRef<A, R>>,
    },
}

pub struct Node<A: Measured, R: FingerTreeRefs<A>> {
    measure: A::Measure,
    inner: NodeInner<A, R>,
}

// 论文中的类型大致是：
//
// FingerTree v a = Empty | Single a | Deep v (Digit a) (FingerTree v (Node v a)) (Digit a)
// Node v a       = Node2 v a a | Node3 v a a a
// Digit a        = One a | Two a a | Three a a a | Four a a a a
//
// Rust 不能直接表达 `a` 到 `Node v a` 的递归类型变化，除非引入更重的编码。
// 这里把所有逻辑层级都存在同一个 `Tree<A, R>` 里，并用 `NodeInner` 动态区分
// 节点载荷。下面这些构造器维持论文里的不变量：
//
// - 对外的根树逻辑元素类型是 `A`，因此节点一定是 `Leaf`。
// - 所有中间树都只由 `Node::lift` 产生，因此节点一定是
//   `Branch2` 或 `Branch3`。
// - 从中间树递归返回的结果会先展开成 Digit，再回到上一层。
// - Digit 永远非空。
//
// 本文件中的 `unreachable!` 都是在检查这些不变量是否被破坏。按论文的类型系统，
// 这些分支静态上不可构造；在这个紧凑的单态表示里，它们标记了动态边界。
enum NodeInner<A: Measured, R: FingerTreeRefs<A>> {
    Leaf(A),
    Branch2 {
        left: NodeRef<A, R>,
        right: NodeRef<A, R>,
    },
    Branch3 {
        left: NodeRef<A, R>,
        middle: NodeRef<A, R>,
        right: NodeRef<A, R>,
    },
}

impl<A, R> Clone for RawFingerTree<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
        }
    }
}

impl<A, R> Clone for Tree<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<A, R> Clone for TreeInner<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    fn clone(&self) -> Self {
        match self {
            Self::Empty => Self::Empty,
            Self::Single { measure, node } => Self::Single {
                measure: measure.clone(),
                node: node.clone(),
            },
            Self::Deep {
                measure,
                prefix,
                deeper,
                suffix,
            } => Self::Deep {
                measure: measure.clone(),
                prefix: prefix.clone(),
                deeper: deeper.clone(),
                suffix: suffix.clone(),
            },
        }
    }
}

impl<A, R> Clone for Node<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    fn clone(&self) -> Self {
        Self {
            measure: self.measure.clone(),
            inner: self.inner.clone(),
        }
    }
}

impl<A, R> Clone for NodeInner<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    fn clone(&self) -> Self {
        match self {
            Self::Leaf(value) => Self::Leaf(value.clone()),
            Self::Branch2 { left, right } => Self::Branch2 {
                left: left.clone(),
                right: right.clone(),
            },
            Self::Branch3 {
                left,
                middle,
                right,
            } => Self::Branch3 {
                left: left.clone(),
                middle: middle.clone(),
                right: right.clone(),
            },
        }
    }
}

#[doc(hidden)]
#[derive(Clone)]
pub enum Digit<A> {
    One([A; 1]),
    Two([A; 2]),
    Three([A; 3]),
    Four([A; 4]),
}

#[doc(hidden)]
pub struct DigitIter<A>(Option<Digit<A>>);

struct NodeList<A: Measured, R: FingerTreeRefs<A>> {
    items: [Option<NodeRef<A, R>>; 8],
    len: usize,
    measure: A::Measure,
}

struct NodeListIter<A: Measured, R: FingerTreeRefs<A>> {
    items: [Option<NodeRef<A, R>>; 8],
    front: usize,
    back: usize,
}

struct LiftNodeIter<'a, A, R, I>
where
    A: Measured,
    R: FingerTreeRefs<A>,
    I: Iterator<Item = NodeRef<A, R>>,
{
    buf: [Option<NodeRef<A, R>>; 5],
    live: u8,
    cursor: u8,
    iter: I,
    refs: &'a mut R,
}

impl<A, R> RawFingerTree<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    #[inline]
    pub fn new() -> Self {
        Self {
            root: Tree::empty(),
        }
    }

    #[inline]
    pub fn from_iter_in<T>(refs: &mut R, iter: T) -> Self
    where
        T: IntoIterator<Item = A>,
    {
        let mut root = Tree::empty();
        for value in iter {
            let node = refs.alloc_node(Node::leaf(value));
            root.push_back_mut(refs, node);
        }
        Self { root }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        matches!(self.root.0, TreeInner::Empty)
    }

    #[inline]
    pub fn measure(&self) -> A::Measure {
        self.root.measure()
    }

    #[inline]
    pub fn front(&self, refs: &R) -> Option<A> {
        self.root
            .front_node()
            .map(|node| Node::leaf_value(refs, &node))
    }

    #[inline]
    pub fn back(&self, refs: &R) -> Option<A> {
        self.root
            .back_node()
            .map(|node| Node::leaf_value(refs, &node))
    }

    #[inline]
    pub fn push_front_mut(&mut self, refs: &mut R, value: A) {
        let node = refs.alloc_node(Node::leaf(value));
        self.root.push_front_mut(refs, node);
    }

    #[inline]
    pub fn push_back_mut(&mut self, refs: &mut R, value: A) {
        let node = refs.alloc_node(Node::leaf(value));
        self.root.push_back_mut(refs, node);
    }

    #[inline]
    pub fn concat(&self, refs: &mut R, other: &Self) -> Self {
        Self {
            root: Tree::concat_ref(refs, &self.root, &other.root),
        }
    }

    #[inline]
    pub fn concat_mut(&mut self, refs: &mut R, other: Self) {
        let left = self.take_root();
        self.root = Tree::concat(refs, left, other.root);
    }

    #[inline]
    pub fn pop_front(&mut self, refs: &mut R) -> Option<A> {
        let old = self.take_root();
        let (head, rest) = Tree::view_front_with(refs, old.0)?;
        self.root = rest;
        Some(Node::leaf_value(refs, &head))
    }

    #[inline]
    pub fn pop_back(&mut self, refs: &mut R) -> Option<A> {
        let old = self.take_root();
        let (rest, last) = Tree::view_back_with(refs, old.0)?;
        self.root = rest;
        Some(Node::leaf_value(refs, &last))
    }

    #[inline]
    pub fn split<F>(&self, refs: &mut R, pred: F) -> Option<(Self, A, Self)>
    where
        F: Fn(&A::Measure) -> bool,
    {
        let (front, mid, back) =
            Tree::split_offset_with(refs, self.root.0.clone(), A::Measure::empty(), &pred)?;
        let mid = Node::leaf_value(refs, &mid);
        Some((Self { root: front }, mid, Self { root: back }))
    }

    #[inline]
    pub fn into_split<F>(self, refs: &mut R, pred: F) -> Option<(Self, A, Self)>
    where
        F: Fn(&A::Measure) -> bool,
    {
        let (front, mid, back) =
            Tree::split_offset_with(refs, self.root.0, A::Measure::empty(), &pred)?;
        let mid = Node::leaf_value(refs, &mid);
        Some((Self { root: front }, mid, Self { root: back }))
    }

    #[inline]
    fn take_root(&mut self) -> Tree<A, R> {
        std::mem::replace(&mut self.root, Tree::empty())
    }

    #[inline]
    fn map_refs<S, FN, FT>(&self, node_map: &FN, tree_map: &FT) -> RawFingerTree<A, S>
    where
        S: FingerTreeRefs<A>,
        FN: Fn(&NodeRef<A, R>) -> NodeRef<A, S>,
        FT: Fn(&TreeRef<A, R>) -> TreeRef<A, S>,
    {
        RawFingerTree {
            root: self.root.map_refs(node_map, tree_map),
        }
    }
}

impl<A, R> Default for RawFingerTree<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<A, R> Measured for RawFingerTree<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    type Measure = A::Measure;
    fn measure(&self) -> Self::Measure {
        self.root.measure()
    }
}

impl<A> FromIterator<A> for RawFingerTree<A>
where
    A: Measured,
{
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let mut refs = FingerTreeStore::default();
        Self::from_iter_in(&mut refs, iter)
    }
}

impl<A, R> Measured for Tree<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    type Measure = A::Measure;
    fn measure(&self) -> Self::Measure {
        match &self.0 {
            TreeInner::Empty => A::Measure::empty(),
            TreeInner::Single { measure, .. } | TreeInner::Deep { measure, .. } => measure.clone(),
        }
    }
}

impl<A, R> Tree<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    fn empty() -> Self {
        Self(TreeInner::Empty)
    }
    fn single(refs: &R, node: NodeRef<A, R>) -> Self {
        Self(TreeInner::Single {
            measure: node_measure(refs, &node),
            node,
        })
    }
    fn deep(
        refs: &R,
        prefix: Digit<NodeRef<A, R>>,
        deeper: TreeRef<A, R>,
        suffix: Digit<NodeRef<A, R>>,
    ) -> Self {
        let measure = digit_measure(&prefix, refs)
            .merge(tree_measure(refs, &deeper))
            .merge(digit_measure(&suffix, refs));
        Self::deep_with_measure(measure, prefix, deeper, suffix)
    }
    fn deep_with_measure(
        measure: A::Measure,
        prefix: Digit<NodeRef<A, R>>,
        deeper: TreeRef<A, R>,
        suffix: Digit<NodeRef<A, R>>,
    ) -> Self {
        Self(TreeInner::Deep {
            measure,
            prefix,
            deeper,
            suffix,
        })
    }
    fn clone_inner_from_ref(refs: &R, tree: &TreeRef<A, R>) -> TreeInner<A, R> {
        refs.clone_tree_ref(tree).0
    }
    fn front_node(&self) -> Option<NodeRef<A, R>> {
        match &self.0 {
            TreeInner::Empty => None,
            TreeInner::Single { node, .. } => Some(node.clone()),
            TreeInner::Deep { prefix, .. } => prefix.first(),
        }
    }
    fn back_node(&self) -> Option<NodeRef<A, R>> {
        match &self.0 {
            TreeInner::Empty => None,
            TreeInner::Single { node, .. } => Some(node.clone()),
            TreeInner::Deep { suffix, .. } => suffix.last(),
        }
    }

    fn push_front(self, refs: &mut R, node: NodeRef<A, R>) -> Self {
        match self.0 {
            TreeInner::Empty => Self::single(refs, node),
            TreeInner::Single { measure, node: old } => {
                let deeper = refs.alloc_tree(Self::empty());
                Self::deep_with_measure(
                    node_measure(refs, &node).merge(measure),
                    Digit::One([node]),
                    deeper,
                    Digit::One([old]),
                )
            }
            TreeInner::Deep {
                measure,
                prefix: Digit::Four([a, b, c, d]),
                deeper,
                suffix,
            } => {
                let branch = Node::branch3(refs, b, c, d);
                let branch = refs.alloc_node(branch);
                let deeper_tree = refs.clone_tree_ref(&deeper);
                let deeper_tree = deeper_tree.push_front(refs, branch);
                let deeper = refs.alloc_tree(deeper_tree);
                Self::deep_with_measure(
                    node_measure(refs, &node).merge(measure),
                    Digit::Two([node, a]),
                    deeper,
                    suffix,
                )
            }
            TreeInner::Deep {
                measure,
                prefix,
                deeper,
                suffix,
            } => Self::deep_with_measure(
                node_measure(refs, &node).merge(measure),
                prefix.push_front(node),
                deeper,
                suffix,
            ),
        }
    }

    fn push_front_mut(&mut self, refs: &mut R, node: NodeRef<A, R>) {
        if matches!(self.0, TreeInner::Empty) {
            *self = Self::single(refs, node);
            return;
        }

        let old = std::mem::replace(self, Self::empty());
        *self = old.push_front(refs, node);
    }

    fn push_back(self, refs: &mut R, node: NodeRef<A, R>) -> Self {
        match self.0 {
            TreeInner::Empty => Self::single(refs, node),
            TreeInner::Single { measure, node: old } => {
                let deeper = refs.alloc_tree(Self::empty());
                Self::deep_with_measure(
                    measure.merge(node_measure(refs, &node)),
                    Digit::One([old]),
                    deeper,
                    Digit::One([node]),
                )
            }
            TreeInner::Deep {
                measure,
                prefix,
                deeper,
                suffix: Digit::Four([a, b, c, d]),
            } => {
                let branch = Node::branch3(refs, a, b, c);
                let branch = refs.alloc_node(branch);
                let deeper_tree = refs.clone_tree_ref(&deeper);
                let deeper_tree = deeper_tree.push_back(refs, branch);
                let deeper = refs.alloc_tree(deeper_tree);
                Self::deep_with_measure(
                    measure.merge(node_measure(refs, &node)),
                    prefix,
                    deeper,
                    Digit::Two([d, node]),
                )
            }
            TreeInner::Deep {
                measure,
                prefix,
                deeper,
                suffix,
            } => Self::deep_with_measure(
                measure.merge(node_measure(refs, &node)),
                prefix,
                deeper,
                suffix.push_back(node),
            ),
        }
    }

    fn push_back_mut(&mut self, refs: &mut R, node: NodeRef<A, R>) {
        if matches!(self.0, TreeInner::Empty) {
            *self = Self::single(refs, node);
            return;
        }

        let old = std::mem::replace(self, Self::empty());
        *self = old.push_back(refs, node);
    }
    fn from_nodes<I>(refs: &mut R, iter: I) -> Self
    where
        I: IntoIterator<Item = NodeRef<A, R>>,
    {
        let mut tree = Self::empty();
        for node in iter {
            tree = tree.push_back(refs, node);
        }
        tree
    }
    fn from_optional_digit(refs: &mut R, digit: Option<Digit<NodeRef<A, R>>>) -> Self {
        digit.map_or(Self::empty(), |digit| Self::from_nodes(refs, digit))
    }

    fn view_front_with(refs: &mut R, layer: TreeInner<A, R>) -> Option<(NodeRef<A, R>, Self)> {
        match layer {
            TreeInner::Empty => None,
            TreeInner::Single { node, .. } => Some((node, Self::empty())),
            TreeInner::Deep {
                prefix,
                deeper,
                suffix,
                ..
            } => {
                let (head, tail) = prefix.view_front();
                Some((head, Self::deep_left_with(refs, tail, deeper, suffix)))
            }
        }
    }

    fn view_back_with(refs: &mut R, layer: TreeInner<A, R>) -> Option<(Self, NodeRef<A, R>)> {
        match layer {
            TreeInner::Empty => None,
            TreeInner::Single { node, .. } => Some((Self::empty(), node)),
            TreeInner::Deep {
                prefix,
                deeper,
                suffix,
                ..
            } => {
                let (init, last) = suffix.view_back();
                Some((Self::deep_right_with(refs, prefix, deeper, init), last))
            }
        }
    }

    fn deep_left_with(
        refs: &mut R,
        prefix: Option<Digit<NodeRef<A, R>>>,
        deeper: TreeRef<A, R>,
        suffix: Digit<NodeRef<A, R>>,
    ) -> Self {
        match prefix {
            Some(prefix) => Self::deep(refs, prefix, deeper, suffix),
            None => match Self::view_front_with(refs, Tree::clone_inner_from_ref(refs, &deeper)) {
                Some((node, rest)) => {
                    let prefix = Node::to_digit(refs, &node);
                    let deeper = refs.alloc_tree(rest);
                    Self::deep(refs, prefix, deeper, suffix)
                }
                None => Self::from_nodes(refs, suffix),
            },
        }
    }

    fn deep_right_with(
        refs: &mut R,
        prefix: Digit<NodeRef<A, R>>,
        deeper: TreeRef<A, R>,
        suffix: Option<Digit<NodeRef<A, R>>>,
    ) -> Self {
        match suffix {
            Some(suffix) => Self::deep(refs, prefix, deeper, suffix),
            None => match Self::view_back_with(refs, Tree::clone_inner_from_ref(refs, &deeper)) {
                Some((rest, node)) => {
                    let deeper = refs.alloc_tree(rest);
                    let suffix = Node::to_digit(refs, &node);
                    Self::deep(refs, prefix, deeper, suffix)
                }
                None => Self::from_nodes(refs, prefix),
            },
        }
    }

    fn split_offset_with<F>(
        refs: &mut R,
        layer: TreeInner<A, R>,
        offset: A::Measure,
        pred: &F,
    ) -> Option<(Self, NodeRef<A, R>, Self)>
    where
        F: Fn(&A::Measure) -> bool,
    {
        match layer {
            TreeInner::Empty => None,
            TreeInner::Single { node, .. } => pred(&offset.merge(node_measure(refs, &node)))
                .then_some((Self::empty(), node, Self::empty())),
            TreeInner::Deep {
                prefix,
                deeper,
                suffix,
                ..
            } => {
                let after_prefix = offset.clone().merge(digit_measure(&prefix, refs));
                if pred(&after_prefix) {
                    let (before, node, after) =
                        digit_split_offset(prefix, offset, pred, refs).unwrap();
                    return Some((
                        Self::from_optional_digit(refs, before),
                        node,
                        Self::deep_left_with(refs, after, deeper, suffix),
                    ));
                }

                let after_deeper = after_prefix.clone().merge(tree_measure(refs, &deeper));
                if pred(&after_deeper) {
                    let (before, branch, after) = Self::split_offset_with(
                        refs,
                        Tree::clone_inner_from_ref(refs, &deeper),
                        after_prefix.clone(),
                        pred,
                    )
                    .unwrap();
                    let (inner_before, node, inner_after) = digit_split_offset(
                        Node::to_digit(refs, &branch),
                        after_prefix.merge(before.measure()),
                        pred,
                        refs,
                    )
                    .unwrap();
                    let before = refs.alloc_tree(before);
                    let front = Self::deep_right_with(refs, prefix, before, inner_before);
                    let after = refs.alloc_tree(after);
                    let back = Self::deep_left_with(refs, inner_after, after, suffix);
                    return Some((front, node, back));
                }

                let (before, node, after) = digit_split_offset(suffix, after_deeper, pred, refs)?;
                Some((
                    Self::deep_right_with(refs, prefix, deeper, before),
                    node,
                    Self::from_optional_digit(refs, after),
                ))
            }
        }
    }
    fn concat(refs: &mut R, front: Self, back: Self) -> Self {
        Self::concat_with_middle(refs, front.0, NodeList::new(), back.0)
    }

    fn concat_ref(refs: &mut R, front: &Self, back: &Self) -> Self {
        Self::concat_with_middle(refs, front.0.clone(), NodeList::new(), back.0.clone())
    }

    fn concat_with_middle(
        refs: &mut R,
        front: TreeInner<A, R>,
        mid: NodeList<A, R>,
        back: TreeInner<A, R>,
    ) -> Self {
        match (front, back) {
            (TreeInner::Empty, back) => Self::push_many_front(refs, mid, Self(back)),
            (front, TreeInner::Empty) => Self::push_many_back(refs, Self(front), mid),
            (TreeInner::Single { node, .. }, back) => {
                Self::push_many_front(refs, mid, Self(back)).push_front(refs, node)
            }
            (front, TreeInner::Single { node, .. }) => {
                Self::push_many_back(refs, Self(front), mid).push_back(refs, node)
            }
            (
                TreeInner::Deep {
                    measure: left_measure,
                    prefix: left_prefix,
                    deeper: left_deeper,
                    suffix: left_suffix,
                },
                TreeInner::Deep {
                    measure: right_measure,
                    prefix: right_prefix,
                    deeper: right_deeper,
                    suffix: right_suffix,
                },
            ) => {
                let NodeList {
                    items: mid_items,
                    len: mid_len,
                    measure: mid_measure,
                } = mid;
                let measure = left_measure.merge(mid_measure).merge(right_measure);
                let left_deeper = Tree::clone_inner_from_ref(refs, &left_deeper);
                let right_deeper = Tree::clone_inner_from_ref(refs, &right_deeper);
                let mid: NodeListIter<A, R> = NodeListIter {
                    items: mid_items,
                    front: 0,
                    back: mid_len,
                };
                let mid =
                    Node::lift_list(refs, left_suffix.into_iter().chain(mid).chain(right_prefix));
                let deeper = Self::concat_with_middle(refs, left_deeper, mid, right_deeper);
                let deeper = refs.alloc_tree(deeper);
                Self::deep_with_measure(measure, left_prefix, deeper, right_suffix)
            }
        }
    }

    fn push_many_front(refs: &mut R, nodes: NodeList<A, R>, mut tree: Self) -> Self {
        for node in nodes.into_iter().rev() {
            tree = tree.push_front(refs, node);
        }
        tree
    }

    fn push_many_back(refs: &mut R, mut tree: Self, nodes: NodeList<A, R>) -> Self {
        for node in nodes {
            tree = tree.push_back(refs, node);
        }
        tree
    }

    fn map_refs<S, FN, FT>(&self, node_map: &FN, tree_map: &FT) -> Tree<A, S>
    where
        S: FingerTreeRefs<A>,
        FN: Fn(&NodeRef<A, R>) -> NodeRef<A, S>,
        FT: Fn(&TreeRef<A, R>) -> TreeRef<A, S>,
    {
        Tree(match &self.0 {
            TreeInner::Empty => TreeInner::Empty,
            TreeInner::Single { measure, node } => TreeInner::Single {
                measure: measure.clone(),
                node: node_map(node),
            },
            TreeInner::Deep {
                measure,
                prefix,
                deeper,
                suffix,
            } => TreeInner::Deep {
                measure: measure.clone(),
                prefix: prefix.map(node_map),
                deeper: tree_map(deeper),
                suffix: suffix.map(node_map),
            },
        })
    }
}

impl<A, R> Node<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    fn leaf(value: A) -> Self {
        Self {
            measure: value.measure(),
            inner: NodeInner::Leaf(value),
        }
    }
    fn branch2(refs: &R, left: NodeRef<A, R>, right: NodeRef<A, R>) -> Self {
        let measure = node_refs_measure(refs, [&left, &right].into_iter());
        Self {
            measure,
            inner: NodeInner::Branch2 { left, right },
        }
    }
    fn branch3(refs: &R, left: NodeRef<A, R>, middle: NodeRef<A, R>, right: NodeRef<A, R>) -> Self {
        let measure = node_refs_measure(refs, [&left, &middle, &right].into_iter());
        Self {
            measure,
            inner: NodeInner::Branch3 {
                left,
                middle,
                right,
            },
        }
    }
    fn leaf_value(refs: &R, node: &NodeRef<A, R>) -> A {
        refs.leaf_value(node)
    }
    fn to_digit(refs: &R, node: &NodeRef<A, R>) -> Digit<NodeRef<A, R>> {
        refs.node_to_digit(node)
    }
    fn lift<'a, I>(refs: &'a mut R, iter: I) -> LiftNodeIter<'a, A, R, I::IntoIter>
    where
        I: IntoIterator<Item = NodeRef<A, R>>,
    {
        LiftNodeIter::new(refs, iter.into_iter())
    }
    fn lift_list<I>(refs: &mut R, iter: I) -> NodeList<A, R>
    where
        I: IntoIterator<Item = NodeRef<A, R>>,
    {
        let mut nodes = NodeList::new();
        for (node, measure) in Self::lift(refs, iter) {
            nodes.push(node, measure);
        }
        nodes
    }

    fn map_refs<S, F>(&self, node_map: &F) -> Node<A, S>
    where
        S: FingerTreeRefs<A>,
        F: Fn(&NodeRef<A, R>) -> NodeRef<A, S>,
    {
        Node {
            measure: self.measure.clone(),
            inner: match &self.inner {
                NodeInner::Leaf(value) => NodeInner::Leaf(value.clone()),
                NodeInner::Branch2 { left, right } => NodeInner::Branch2 {
                    left: node_map(left),
                    right: node_map(right),
                },
                NodeInner::Branch3 {
                    left,
                    middle,
                    right,
                } => NodeInner::Branch3 {
                    left: node_map(left),
                    middle: node_map(middle),
                    right: node_map(right),
                },
            },
        }
    }
}

impl<A, R> Measured for Node<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    type Measure = A::Measure;
    fn measure(&self) -> Self::Measure {
        self.measure.clone()
    }
}

impl<A> Digit<A> {
    fn as_slice(&self) -> &[A] {
        match self {
            Self::One(items) => items,
            Self::Two(items) => items,
            Self::Three(items) => items,
            Self::Four(items) => items,
        }
    }
    fn first(&self) -> Option<A>
    where
        A: Clone,
    {
        self.as_slice().first().cloned()
    }
    fn last(&self) -> Option<A>
    where
        A: Clone,
    {
        self.as_slice().last().cloned()
    }

    fn push_front(self, value: A) -> Self {
        match self {
            Self::One([a]) => Self::Two([value, a]),
            Self::Two([a, b]) => Self::Three([value, a, b]),
            Self::Three([a, b, c]) => Self::Four([value, a, b, c]),
            // Tree::push_front 遇到满前缀时会把 Node3 推入中间树；
            // digit_split_offset 也不会向满 digit 继续前插。论文里这是由
            // 对 Digit 的模式匹配保证的。
            Self::Four(_) => unreachable!("cannot push into a full digit"),
        }
    }
    fn push_back(self, value: A) -> Self {
        match self {
            Self::One([a]) => Self::Two([a, value]),
            Self::Two([a, b]) => Self::Three([a, b, value]),
            Self::Three([a, b, c]) => Self::Four([a, b, c, value]),
            // Tree::push_back 遇到满后缀时会把 Node3 推入中间树。
            // 合法的 FingerTree 操作不会向 Four 继续后插。
            Self::Four(_) => unreachable!("cannot push into a full digit"),
        }
    }
    fn prepend_to_option(value: A, digit: Option<Self>) -> Self {
        match digit {
            Some(digit) => digit.push_front(value),
            None => Self::One([value]),
        }
    }
    fn view_front(self) -> (A, Option<Self>) {
        match self {
            Self::One([a]) => (a, None),
            Self::Two([a, b]) => (a, Some(Self::One([b]))),
            Self::Three([a, b, c]) => (a, Some(Self::Two([b, c]))),
            Self::Four([a, b, c, d]) => (a, Some(Self::Three([b, c, d]))),
        }
    }
    fn view_back(self) -> (Option<Self>, A) {
        match self {
            Self::One([a]) => (None, a),
            Self::Two([a, b]) => (Some(Self::One([a])), b),
            Self::Three([a, b, c]) => (Some(Self::Two([a, b])), c),
            Self::Four([a, b, c, d]) => (Some(Self::Three([a, b, c])), d),
        }
    }

    fn map<B, F>(&self, f: &F) -> Digit<B>
    where
        F: Fn(&A) -> B,
    {
        match self {
            Self::One([a]) => Digit::One([f(a)]),
            Self::Two([a, b]) => Digit::Two([f(a), f(b)]),
            Self::Three([a, b, c]) => Digit::Three([f(a), f(b), f(c)]),
            Self::Four([a, b, c, d]) => Digit::Four([f(a), f(b), f(c), f(d)]),
        }
    }
}

impl<A> IntoIterator for Digit<A> {
    type Item = A;
    type IntoIter = DigitIter<A>;
    fn into_iter(self) -> Self::IntoIter {
        DigitIter(Some(self))
    }
}

impl<A> Iterator for DigitIter<A> {
    type Item = A;
    fn next(&mut self) -> Option<Self::Item> {
        match self.0.take() {
            None => None,
            Some(Digit::One([a])) => Some(a),
            Some(Digit::Two([a, b])) => {
                self.0 = Some(Digit::One([b]));
                Some(a)
            }
            Some(Digit::Three([a, b, c])) => {
                self.0 = Some(Digit::Two([b, c]));
                Some(a)
            }
            Some(Digit::Four([a, b, c, d])) => {
                self.0 = Some(Digit::Three([b, c, d]));
                Some(a)
            }
        }
    }
}

impl<A, R> NodeList<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    fn new() -> Self {
        Self {
            items: std::array::from_fn(|_| None),
            len: 0,
            measure: A::Measure::empty(),
        }
    }
    fn push(&mut self, node: NodeRef<A, R>, measure: A::Measure) {
        // concat 中被提升的节点来自 suffix ++ middle ++ prefix。按论文的
        // `nodes` 分组规则，结果至多 4 个；这里留 8 个槽是为了让这个动态编码
        // 在未来微调分组时仍有余量。
        debug_assert!(self.len < self.items.len());
        append_measure(&mut self.measure, measure);
        self.items[self.len] = Some(node);
        self.len += 1;
    }
}

impl<A, R> IntoIterator for NodeList<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    type Item = NodeRef<A, R>;
    type IntoIter = NodeListIter<A, R>;

    fn into_iter(self) -> Self::IntoIter {
        NodeListIter {
            items: self.items,
            front: 0,
            back: self.len,
        }
    }
}

impl<A, R> Iterator for NodeListIter<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    type Item = NodeRef<A, R>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let node = self.items[self.front].take();
        self.front += 1;
        node
    }
}

impl<A, R> DoubleEndedIterator for NodeListIter<A, R>
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.items[self.back].take()
    }
}

impl<'a, A, R, I> LiftNodeIter<'a, A, R, I>
where
    A: Measured,
    R: FingerTreeRefs<A>,
    I: Iterator<Item = NodeRef<A, R>>,
{
    fn new(refs: &'a mut R, mut iter: I) -> Self {
        let buf = [
            iter.next(),
            iter.next(),
            iter.next(),
            iter.next(),
            iter.next(),
        ];
        let live = buf.iter().filter(|node| node.is_some()).count() as u8;
        Self {
            buf,
            live,
            cursor: 0,
            iter,
            refs,
        }
    }

    fn pop_buffered(&mut self) -> NodeRef<A, R> {
        let next = self.iter.next();
        if next.is_none() {
            self.live -= 1;
        }
        let node = core::mem::replace(&mut self.buf[self.cursor as usize], next).unwrap();
        self.cursor = (self.cursor + 1) % 5;
        node
    }
}

impl<A, R, I> Iterator for LiftNodeIter<'_, A, R, I>
where
    A: Measured,
    R: FingerTreeRefs<A>,
    I: Iterator<Item = NodeRef<A, R>>,
{
    type Item = (NodeRef<A, R>, A::Measure);

    fn next(&mut self) -> Option<Self::Item> {
        match self.live {
            0 => None,
            2 | 4 => {
                let left = self.pop_buffered();
                let right = self.pop_buffered();
                let node = Node::branch2(self.refs, left, right);
                let measure = node.measure.clone();
                Some((self.refs.alloc_node(node), measure))
            }
            3 | 5 => {
                let left = self.pop_buffered();
                let middle = self.pop_buffered();
                let right = self.pop_buffered();
                let node = Node::branch3(self.refs, left, middle, right);
                let measure = node.measure.clone();
                Some((self.refs.alloc_node(node), measure))
            }
            // LiftNodeIter 对应论文里的 `nodes` 辅助函数。它只在 concat 时处理
            // 左侧后缀 ++ 中间节点 ++ 右侧前缀；两侧 digit 都非空，所以输入
            // 长度至少为二。5 槽前瞻缓冲会把流分组成 Node2/Node3，不会留下
            // 单个尾元素。
            _ => unreachable!("cannot lift one remaining node"),
        }
    }
}

fn node_measure<A, R>(refs: &R, node: &NodeRef<A, R>) -> A::Measure
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    refs.measure_node_ref(node)
}

fn append_measure<M: Monoid>(measure: &mut M, other: M) {
    let old = std::mem::replace(measure, M::empty());
    *measure = old.merge(other);
}

fn prepend_measure<M: Monoid>(measure: &mut M, other: M) {
    let old = std::mem::replace(measure, M::empty());
    *measure = other.merge(old);
}

fn tree_measure<A, R>(refs: &R, tree: &TreeRef<A, R>) -> A::Measure
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    refs.measure_tree_ref(tree)
}

fn digit_measure<A, R>(digit: &Digit<NodeRef<A, R>>, refs: &R) -> A::Measure
where
    A: Measured,
    R: FingerTreeRefs<A>,
{
    node_refs_measure(refs, digit.as_slice().iter())
}

fn node_refs_measure<'a, A, R>(
    refs: &R,
    mut nodes: impl Iterator<Item = &'a NodeRef<A, R>>,
) -> A::Measure
where
    A: Measured,
    R: FingerTreeRefs<A>,
    NodeRef<A, R>: 'a,
{
    // 调用方传入的只会是 Digit、Node2 或 Node3；它们在论文的数据类型里都非空，
    // 当前表示也维持同样的不变量。
    let Some(first) = nodes.next() else {
        unreachable!("node list is never empty");
    };
    nodes.fold(node_measure(refs, first), |measure, node| {
        measure.merge(node_measure(refs, node))
    })
}

fn digit_split_offset<A, R, F>(
    digit: Digit<NodeRef<A, R>>,
    offset: A::Measure,
    pred: &F,
    refs: &R,
) -> Option<DigitSplit<A, R>>
where
    A: Measured,
    R: FingerTreeRefs<A>,
    F: Fn(&A::Measure) -> bool,
{
    let (head, tail) = digit.view_front();
    let after_head = offset.merge(node_measure(refs, &head));
    if pred(&after_head) {
        Some((None, head, tail))
    } else {
        let (before, node, after) = digit_split_offset(tail?, after_head, pred, refs)?;
        Some((Some(Digit::prepend_to_option(head, before)), node, after))
    }
}
