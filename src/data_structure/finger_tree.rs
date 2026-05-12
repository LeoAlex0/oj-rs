use std::{cell::RefCell, rc::Rc};

use crate::traits::{monoid::Monoid, monoid::Size, semigroup::Semigroup};

pub mod prelude {
    pub use super::{
        ArenaFamily, ArenaFingerTree, BoxFamily, BoxFingerTree, FingerTree, HeapRefKind, Measured,
        RcFamily, RefFamily, Value,
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

pub trait RefFamily<A: Measured>: Clone + Sized {
    type NodeRef: Clone;
    type TreeRef: Clone;

    fn alloc_node(&self, node: Node<A, Self>) -> Self::NodeRef;
    fn alloc_tree(&self, tree: Tree<A, Self>) -> Self::TreeRef;

    fn with_node<T, F>(&self, node: &Self::NodeRef, f: F) -> T
    where
        F: FnOnce(&Node<A, Self>) -> T;

    fn with_tree<T, F>(&self, tree: &Self::TreeRef, f: F) -> T
    where
        F: FnOnce(&Tree<A, Self>) -> T;

    fn into_node(&self, node: Self::NodeRef) -> Node<A, Self>;
    fn into_tree(&self, tree: Self::TreeRef) -> Tree<A, Self>;

    // 判断 `self` 分配出来的引用能否被 `other` 解释，反过来也一样。
    //
    // 这不是值相等，而是分配来源检查。concat 会把两边的节点接进同一棵
    // 结果树里，所以必须确认两边的引用句柄属于同一个可解释区域。
    // Rc/Box 这类堆引用自己携带目标地址，同一种 family 的任意值都兼容。
    // ArenaFamily 只保存数字下标，因此只有指向同一个底层存储时才能拼接。
    fn same_region(&self, _other: &Self) -> bool {
        true
    }
}

#[derive(Clone, Copy, Default)]
pub struct RcFamily;

#[derive(Clone, Copy, Default)]
pub struct BoxFamily;

pub trait HeapRefKind: Clone + Copy + Default {
    type Ref<T: Clone>: Clone + AsRef<T>;

    fn new_ref<T: Clone>(value: T) -> Self::Ref<T>;
    fn into_owned<T: Clone>(value: Self::Ref<T>) -> T;
}

impl HeapRefKind for RcFamily {
    type Ref<T: Clone> = Rc<T>;

    fn new_ref<T: Clone>(value: T) -> Self::Ref<T> {
        Rc::new(value)
    }

    fn into_owned<T: Clone>(value: Self::Ref<T>) -> T {
        match Rc::try_unwrap(value) {
            Ok(value) => value,
            Err(value) => value.as_ref().clone(),
        }
    }
}

impl HeapRefKind for BoxFamily {
    type Ref<T: Clone> = Box<T>;

    fn new_ref<T: Clone>(value: T) -> Self::Ref<T> {
        Box::new(value)
    }

    fn into_owned<T: Clone>(value: Self::Ref<T>) -> T {
        *value
    }
}

impl<A, R> RefFamily<A> for R
where
    A: Measured,
    R: HeapRefKind,
{
    type NodeRef = R::Ref<Node<A, R>>;
    type TreeRef = R::Ref<Tree<A, R>>;

    fn alloc_node(&self, node: Node<A, Self>) -> Self::NodeRef {
        R::new_ref(node)
    }

    fn alloc_tree(&self, tree: Tree<A, Self>) -> Self::TreeRef {
        R::new_ref(tree)
    }

    fn with_node<T, F>(&self, node: &Self::NodeRef, f: F) -> T
    where
        F: FnOnce(&Node<A, Self>) -> T,
    {
        f(node.as_ref())
    }

    fn with_tree<T, F>(&self, tree: &Self::TreeRef, f: F) -> T
    where
        F: FnOnce(&Tree<A, Self>) -> T,
    {
        f(tree.as_ref())
    }

    fn into_node(&self, node: Self::NodeRef) -> Node<A, Self> {
        R::into_owned(node)
    }

    fn into_tree(&self, tree: Self::TreeRef) -> Tree<A, Self> {
        R::into_owned(tree)
    }
}

pub type BoxFingerTree<A> = FingerTree<A, BoxFamily>;
pub type ArenaFingerTree<A> = FingerTree<A, ArenaFamily<A>>;

pub struct ArenaFamily<A: Measured> {
    storage: Rc<RefCell<ArenaStorage<A>>>,
}

struct ArenaStorage<A: Measured> {
    nodes: Vec<Node<A, ArenaFamily<A>>>,
    trees: Vec<Tree<A, ArenaFamily<A>>>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ArenaNodeRef {
    index: usize,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ArenaTreeRef {
    index: usize,
}

impl<A: Measured> Clone for ArenaFamily<A> {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
        }
    }
}

impl<A: Measured> ArenaFamily<A> {
    pub fn new() -> Self {
        Self::with_capacity(0)
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            storage: Rc::new(RefCell::new(ArenaStorage {
                nodes: Vec::with_capacity(capacity),
                trees: Vec::with_capacity(capacity / 4 + 1),
            })),
        }
    }
}

impl<A: Measured> Default for ArenaFamily<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Measured> RefFamily<A> for ArenaFamily<A> {
    type NodeRef = ArenaNodeRef;
    type TreeRef = ArenaTreeRef;

    fn alloc_node(&self, node: Node<A, Self>) -> Self::NodeRef {
        let mut storage = self.storage.borrow_mut();
        let index = storage.nodes.len();
        storage.nodes.push(node);
        ArenaNodeRef { index }
    }

    fn alloc_tree(&self, tree: Tree<A, Self>) -> Self::TreeRef {
        let mut storage = self.storage.borrow_mut();
        let index = storage.trees.len();
        storage.trees.push(tree);
        ArenaTreeRef { index }
    }

    fn with_node<T, F>(&self, node: &Self::NodeRef, f: F) -> T
    where
        F: FnOnce(&Node<A, Self>) -> T,
    {
        let storage = self.storage.borrow();
        f(&storage.nodes[node.index])
    }
    fn into_node(&self, node: Self::NodeRef) -> Node<A, Self> {
        self.storage.borrow().nodes[node.index].clone()
    }

    fn with_tree<T, F>(&self, tree: &Self::TreeRef, f: F) -> T
    where
        F: FnOnce(&Tree<A, Self>) -> T,
    {
        let storage = self.storage.borrow();
        f(&storage.trees[tree.index])
    }

    fn into_tree(&self, tree: Self::TreeRef) -> Tree<A, Self> {
        self.storage.borrow().trees[tree.index].clone()
    }

    fn same_region(&self, other: &Self) -> bool {
        // Arena 句柄只是下标；另一个 arena 里的同一个下标可能指向完全无关的
        // 节点，所以来源兼容性就是共享存储的指针身份是否相同。
        Rc::ptr_eq(&self.storage, &other.storage)
    }
}

type NodeRef<A, R> = <R as RefFamily<A>>::NodeRef;
type TreeRef<A, R> = <R as RefFamily<A>>::TreeRef;

#[derive(Clone)]
pub struct FingerTree<A: Measured, R: RefFamily<A> = RcFamily> {
    root: Tree<A, R>,
    refs: R,
}

#[derive(Clone)]
pub struct Tree<A: Measured, R: RefFamily<A>>(TreeInner<A, R>);

#[derive(Clone)]
enum TreeInner<A: Measured, R: RefFamily<A>> {
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

#[derive(Clone)]
pub struct Node<A: Measured, R: RefFamily<A>> {
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
#[derive(Clone)]
enum NodeInner<A: Measured, R: RefFamily<A>> {
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

#[derive(Clone)]
enum Digit<A> {
    One([A; 1]),
    Two([A; 2]),
    Three([A; 3]),
    Four([A; 4]),
}

struct DigitIter<A>(Option<Digit<A>>);

struct LiftNodeIter<A, R, I>
where
    A: Measured,
    R: RefFamily<A>,
    I: Iterator<Item = NodeRef<A, R>>,
{
    buf: [Option<NodeRef<A, R>>; 5],
    live: u8,
    cursor: u8,
    iter: I,
    refs: R,
}

struct OwnedPath;
struct SharedPath;

trait TreePath<A, R>
where
    A: Measured,
    R: RefFamily<A>,
{
    fn tree_inner(tree: TreeRef<A, R>, refs: &R) -> TreeInner<A, R>;
    fn node_digit(node: NodeRef<A, R>, refs: &R) -> Digit<NodeRef<A, R>>;
}

impl<A, R> TreePath<A, R> for OwnedPath
where
    A: Measured,
    R: RefFamily<A>,
{
    fn tree_inner(tree: TreeRef<A, R>, refs: &R) -> TreeInner<A, R> {
        refs.into_tree(tree).0
    }

    fn node_digit(node: NodeRef<A, R>, refs: &R) -> Digit<NodeRef<A, R>> {
        Node::into_digit(node, refs)
    }
}

impl<A, R> TreePath<A, R> for SharedPath
where
    A: Measured,
    R: RefFamily<A>,
{
    fn tree_inner(tree: TreeRef<A, R>, refs: &R) -> TreeInner<A, R> {
        Tree::clone_inner_from_ref(&tree, refs)
    }

    fn node_digit(node: NodeRef<A, R>, refs: &R) -> Digit<NodeRef<A, R>> {
        Node::to_digit(&node, refs)
    }
}

impl<A, R> FingerTree<A, R>
where
    A: Measured,
    R: RefFamily<A> + Default,
{
    pub fn new() -> Self {
        Self::new_in(R::default())
    }
}

impl<A, R> FingerTree<A, R>
where
    A: Measured,
    R: RefFamily<A>,
{
    pub fn new_in(refs: R) -> Self {
        Self {
            root: Tree::empty(),
            refs,
        }
    }
    pub fn is_empty(&self) -> bool {
        matches!(self.root.0, TreeInner::Empty)
    }
    pub fn measure(&self) -> A::Measure {
        self.root.measure()
    }
    pub fn push_front(&self, value: A) -> Self {
        self.clone().into_push_front(value)
    }
    pub fn push_back(&self, value: A) -> Self {
        self.clone().into_push_back(value)
    }
    pub fn push_front_mut(&mut self, value: A) {
        let old = self.take_root();
        self.root = old.push_front(self.refs.alloc_node(Node::leaf(value)), &self.refs);
    }
    pub fn push_back_mut(&mut self, value: A) {
        let old = self.take_root();
        self.root = old.push_back(self.refs.alloc_node(Node::leaf(value)), &self.refs);
    }
    pub fn into_push_front(mut self, value: A) -> Self {
        self.push_front_mut(value);
        self
    }
    pub fn into_push_back(mut self, value: A) -> Self {
        self.push_back_mut(value);
        self
    }
    pub fn concat(&self, other: &Self) -> Self {
        assert!(self.refs.same_region(&other.refs));
        Self {
            root: Tree::concat_ref(&self.root, &other.root, &self.refs),
            refs: self.refs.clone(),
        }
    }
    pub fn concat_mut(&mut self, other: Self) {
        assert!(self.refs.same_region(&other.refs));
        let left = self.take_root();
        self.root = Tree::concat(left, other.root, &self.refs);
    }
    pub fn into_concat(mut self, other: Self) -> Self {
        self.concat_mut(other);
        self
    }
    pub fn view_front(&self) -> Option<(A, Self)> {
        let (head, root) = self.root.view_front_ref(&self.refs)?;
        Some((
            Node::clone_leaf(&head, &self.refs),
            Self {
                root,
                refs: self.refs.clone(),
            },
        ))
    }
    pub fn view_back(&self) -> Option<(Self, A)> {
        let (root, last) = self.root.view_back_ref(&self.refs)?;
        Some((
            Self {
                root,
                refs: self.refs.clone(),
            },
            Node::clone_leaf(&last, &self.refs),
        ))
    }
    pub fn pop_front(&mut self) -> Option<A> {
        let old = self.take_root();
        let (head, rest) = match old.view_front(&self.refs) {
            Some(view) => view,
            None => return None,
        };
        self.root = rest;
        Some(Node::into_leaf(head, &self.refs))
    }
    pub fn pop_back(&mut self) -> Option<A> {
        let old = self.take_root();
        let (rest, last) = match old.view_back(&self.refs) {
            Some(view) => view,
            None => return None,
        };
        self.root = rest;
        Some(Node::into_leaf(last, &self.refs))
    }
    pub fn into_view_front(mut self) -> Option<(A, Self)> {
        self.pop_front().map(|value| (value, self))
    }
    pub fn into_view_back(mut self) -> Option<(Self, A)> {
        self.pop_back().map(|value| (self, value))
    }
    pub fn split<F>(&self, pred: F) -> Option<(Self, A, Self)>
    where
        F: Fn(&A::Measure) -> bool,
    {
        let (front, mid, back) =
            self.root
                .split_offset_ref(A::Measure::empty(), &pred, &self.refs)?;
        let mid = Node::clone_leaf(&mid, &self.refs);
        Some((
            Self {
                root: front,
                refs: self.refs.clone(),
            },
            mid,
            Self {
                root: back,
                refs: self.refs.clone(),
            },
        ))
    }
    pub fn into_split<F>(self, pred: F) -> Option<(Self, A, Self)>
    where
        F: Fn(&A::Measure) -> bool,
    {
        let refs = self.refs;
        let (front, mid, back) = self.root.split_offset(A::Measure::empty(), &pred, &refs)?;
        let mid = Node::into_leaf(mid, &refs);
        Some((
            Self {
                root: front,
                refs: refs.clone(),
            },
            mid,
            Self { root: back, refs },
        ))
    }

    fn take_root(&mut self) -> Tree<A, R> {
        std::mem::replace(&mut self.root, Tree::empty())
    }
}

impl<A: Measured> FingerTree<A, ArenaFamily<A>> {
    pub fn new_arena() -> Self {
        Self::new_in(ArenaFamily::new())
    }
    pub fn with_arena_capacity(capacity: usize) -> Self {
        Self::new_in(ArenaFamily::with_capacity(capacity))
    }
}

impl<A, R> Default for FingerTree<A, R>
where
    A: Measured,
    R: RefFamily<A> + Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<A, R> Measured for FingerTree<A, R>
where
    A: Measured,
    R: RefFamily<A>,
{
    type Measure = A::Measure;
    fn measure(&self) -> Self::Measure {
        self.root.measure()
    }
}

impl<A, R> FromIterator<A> for FingerTree<A, R>
where
    A: Measured,
    R: RefFamily<A> + Default,
{
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let refs = R::default();
        Self {
            root: Tree::from_nodes(
                iter.into_iter()
                    .map(|value| refs.alloc_node(Node::leaf(value))),
                &refs,
            ),
            refs,
        }
    }
}

impl<A, R> Measured for Tree<A, R>
where
    A: Measured,
    R: RefFamily<A>,
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
    R: RefFamily<A>,
{
    fn empty() -> Self {
        Self(TreeInner::Empty)
    }
    fn single(node: NodeRef<A, R>, refs: &R) -> Self {
        Self(TreeInner::Single {
            measure: node_measure(refs, &node),
            node,
        })
    }
    fn deep(
        prefix: Digit<NodeRef<A, R>>,
        deeper: TreeRef<A, R>,
        suffix: Digit<NodeRef<A, R>>,
        refs: &R,
    ) -> Self {
        let measure = digit_measure(&prefix, refs)
            .merge(tree_measure(refs, &deeper))
            .merge(digit_measure(&suffix, refs));
        Self(TreeInner::Deep {
            measure,
            prefix,
            deeper,
            suffix,
        })
    }
    fn clone_inner(&self) -> TreeInner<A, R> {
        self.0.clone()
    }
    fn clone_inner_from_ref(tree: &TreeRef<A, R>, refs: &R) -> TreeInner<A, R> {
        refs.with_tree(tree, |tree| tree.clone_inner())
    }

    fn push_front(self, node: NodeRef<A, R>, refs: &R) -> Self {
        match self.0 {
            TreeInner::Empty => Self::single(node, refs),
            TreeInner::Single { node: old, .. } => Self::deep(
                Digit::One([node]),
                refs.alloc_tree(Self::empty()),
                Digit::One([old]),
                refs,
            ),
            TreeInner::Deep {
                prefix: Digit::Four([a, b, c, d]),
                deeper,
                suffix,
                ..
            } => Self::deep(
                Digit::Two([node, a]),
                refs.alloc_tree(
                    refs.into_tree(deeper)
                        .push_front(refs.alloc_node(Node::branch3(b, c, d, refs)), refs),
                ),
                suffix,
                refs,
            ),
            TreeInner::Deep {
                prefix,
                deeper,
                suffix,
                ..
            } => Self::deep(prefix.push_front(node), deeper, suffix, refs),
        }
    }

    fn push_back(self, node: NodeRef<A, R>, refs: &R) -> Self {
        match self.0 {
            TreeInner::Empty => Self::single(node, refs),
            TreeInner::Single { node: old, .. } => Self::deep(
                Digit::One([old]),
                refs.alloc_tree(Self::empty()),
                Digit::One([node]),
                refs,
            ),
            TreeInner::Deep {
                prefix,
                deeper,
                suffix: Digit::Four([a, b, c, d]),
                ..
            } => Self::deep(
                prefix,
                refs.alloc_tree(
                    refs.into_tree(deeper)
                        .push_back(refs.alloc_node(Node::branch3(a, b, c, refs)), refs),
                ),
                Digit::Two([d, node]),
                refs,
            ),
            TreeInner::Deep {
                prefix,
                deeper,
                suffix,
                ..
            } => Self::deep(prefix, deeper, suffix.push_back(node), refs),
        }
    }
    fn from_nodes<I>(iter: I, refs: &R) -> Self
    where
        I: IntoIterator<Item = NodeRef<A, R>>,
    {
        iter.into_iter()
            .fold(Self::empty(), |tree, node| tree.push_back(node, refs))
    }
    fn from_optional_digit(digit: Option<Digit<NodeRef<A, R>>>, refs: &R) -> Self {
        digit.map_or(Self::empty(), |digit| Self::from_nodes(digit, refs))
    }

    fn view_front(self, refs: &R) -> Option<(NodeRef<A, R>, Self)> {
        Self::view_front_with::<OwnedPath>(self.0, refs)
    }

    fn view_back(self, refs: &R) -> Option<(Self, NodeRef<A, R>)> {
        Self::view_back_with::<OwnedPath>(self.0, refs)
    }
    fn view_front_ref(&self, refs: &R) -> Option<(NodeRef<A, R>, Self)> {
        Self::view_front_with::<SharedPath>(self.clone_inner(), refs)
    }

    fn view_front_with<P>(layer: TreeInner<A, R>, refs: &R) -> Option<(NodeRef<A, R>, Self)>
    where
        P: TreePath<A, R>,
    {
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
                Some((head, Self::deep_left_with::<P>(tail, deeper, suffix, refs)))
            }
        }
    }
    fn view_back_ref(&self, refs: &R) -> Option<(Self, NodeRef<A, R>)> {
        Self::view_back_with::<SharedPath>(self.clone_inner(), refs)
    }

    fn view_back_with<P>(layer: TreeInner<A, R>, refs: &R) -> Option<(Self, NodeRef<A, R>)>
    where
        P: TreePath<A, R>,
    {
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
                Some((Self::deep_right_with::<P>(prefix, deeper, init, refs), last))
            }
        }
    }

    fn deep_left_with<P>(
        prefix: Option<Digit<NodeRef<A, R>>>,
        deeper: TreeRef<A, R>,
        suffix: Digit<NodeRef<A, R>>,
        refs: &R,
    ) -> Self
    where
        P: TreePath<A, R>,
    {
        match prefix {
            Some(prefix) => Self::deep(prefix, deeper, suffix, refs),
            None => match Self::view_front_with::<P>(P::tree_inner(deeper, refs), refs) {
                Some((node, rest)) => Self::deep(
                    P::node_digit(node, refs),
                    refs.alloc_tree(rest),
                    suffix,
                    refs,
                ),
                None => Self::from_nodes(suffix, refs),
            },
        }
    }

    fn deep_right_with<P>(
        prefix: Digit<NodeRef<A, R>>,
        deeper: TreeRef<A, R>,
        suffix: Option<Digit<NodeRef<A, R>>>,
        refs: &R,
    ) -> Self
    where
        P: TreePath<A, R>,
    {
        match suffix {
            Some(suffix) => Self::deep(prefix, deeper, suffix, refs),
            None => match Self::view_back_with::<P>(P::tree_inner(deeper, refs), refs) {
                Some((rest, node)) => Self::deep(
                    prefix,
                    refs.alloc_tree(rest),
                    P::node_digit(node, refs),
                    refs,
                ),
                None => Self::from_nodes(prefix, refs),
            },
        }
    }

    fn split_offset<F>(
        self,
        offset: A::Measure,
        pred: &F,
        refs: &R,
    ) -> Option<(Self, NodeRef<A, R>, Self)>
    where
        F: Fn(&A::Measure) -> bool,
    {
        Self::split_offset_with::<OwnedPath, _>(self.0, offset, pred, refs)
    }

    fn split_offset_ref<F>(
        &self,
        offset: A::Measure,
        pred: &F,
        refs: &R,
    ) -> Option<(Self, NodeRef<A, R>, Self)>
    where
        F: Fn(&A::Measure) -> bool,
    {
        Self::split_offset_with::<SharedPath, _>(self.clone_inner(), offset, pred, refs)
    }

    fn split_offset_with<P, F>(
        layer: TreeInner<A, R>,
        offset: A::Measure,
        pred: &F,
        refs: &R,
    ) -> Option<(Self, NodeRef<A, R>, Self)>
    where
        P: TreePath<A, R>,
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
                        Self::from_optional_digit(before, refs),
                        node,
                        Self::deep_left_with::<P>(after, deeper, suffix, refs),
                    ));
                }

                let after_deeper = after_prefix.clone().merge(tree_measure(refs, &deeper));
                if pred(&after_deeper) {
                    let (before, branch, after) = Self::split_offset_with::<P, _>(
                        P::tree_inner(deeper, refs),
                        after_prefix.clone(),
                        pred,
                        refs,
                    )
                    .unwrap();
                    let (inner_before, node, inner_after) = digit_split_offset(
                        P::node_digit(branch, refs),
                        after_prefix.merge(before.measure()),
                        pred,
                        refs,
                    )
                    .unwrap();
                    return Some((
                        Self::deep_right_with::<P>(
                            prefix,
                            refs.alloc_tree(before),
                            inner_before,
                            refs,
                        ),
                        node,
                        Self::deep_left_with::<P>(
                            inner_after,
                            refs.alloc_tree(after),
                            suffix,
                            refs,
                        ),
                    ));
                }

                let (before, node, after) = digit_split_offset(suffix, after_deeper, pred, refs)?;
                Some((
                    Self::deep_right_with::<P>(prefix, deeper, before, refs),
                    node,
                    Self::from_optional_digit(after, refs),
                ))
            }
        }
    }
    fn concat(front: Self, back: Self, refs: &R) -> Self {
        let mut mid = std::iter::empty();
        Self::concat_with_middle::<OwnedPath>(front.0, &mut mid, back.0, refs)
    }

    fn concat_ref(front: &Self, back: &Self, refs: &R) -> Self {
        let mut mid = std::iter::empty();
        Self::concat_with_middle::<SharedPath>(
            front.clone_inner(),
            &mut mid,
            back.clone_inner(),
            refs,
        )
    }

    fn concat_with_middle<P>(
        front: TreeInner<A, R>,
        mid: &mut dyn Iterator<Item = NodeRef<A, R>>,
        back: TreeInner<A, R>,
        refs: &R,
    ) -> Self
    where
        P: TreePath<A, R>,
    {
        match (front, back) {
            (TreeInner::Empty, back) => Self::push_many_front(mid, Self(back), refs),
            (front, TreeInner::Empty) => Self::push_many_back(Self(front), mid, refs),
            (TreeInner::Single { node, .. }, back) => {
                Self::push_many_front(mid, Self(back), refs).push_front(node, refs)
            }
            (front, TreeInner::Single { node, .. }) => {
                Self::push_many_back(Self(front), mid, refs).push_back(node, refs)
            }
            (
                TreeInner::Deep {
                    prefix: left_prefix,
                    deeper: left_deeper,
                    suffix: left_suffix,
                    ..
                },
                TreeInner::Deep {
                    prefix: right_prefix,
                    deeper: right_deeper,
                    suffix: right_suffix,
                    ..
                },
            ) => Self::deep(
                left_prefix,
                refs.alloc_tree(Self::concat_with_middle::<P>(
                    P::tree_inner(left_deeper, refs),
                    &mut Node::lift(left_suffix.into_iter().chain(mid).chain(right_prefix), refs),
                    P::tree_inner(right_deeper, refs),
                    refs,
                )),
                right_suffix,
                refs,
            ),
        }
    }

    fn push_many_front<I>(iter: &mut I, tree: Self, refs: &R) -> Self
    where
        I: Iterator<Item = NodeRef<A, R>> + ?Sized,
    {
        match iter.next() {
            None => tree,
            Some(node) => Self::push_many_front(iter, tree, refs).push_front(node, refs),
        }
    }

    fn push_many_back<I>(mut tree: Self, iter: &mut I, refs: &R) -> Self
    where
        I: Iterator<Item = NodeRef<A, R>> + ?Sized,
    {
        for node in iter {
            tree = tree.push_back(node, refs);
        }
        tree
    }
}

impl<A, R> Node<A, R>
where
    A: Measured,
    R: RefFamily<A>,
{
    fn leaf(value: A) -> Self {
        Self {
            measure: value.measure(),
            inner: NodeInner::Leaf(value),
        }
    }
    fn branch2(left: NodeRef<A, R>, right: NodeRef<A, R>, refs: &R) -> Self {
        let measure = node_refs_measure(refs, [&left, &right].into_iter());
        Self {
            measure,
            inner: NodeInner::Branch2 { left, right },
        }
    }
    fn branch3(left: NodeRef<A, R>, middle: NodeRef<A, R>, right: NodeRef<A, R>, refs: &R) -> Self {
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
    fn into_leaf(node: NodeRef<A, R>, refs: &R) -> A {
        match refs.into_node(node).inner {
            NodeInner::Leaf(value) => value,
            NodeInner::Branch2 { .. } | NodeInner::Branch3 { .. } => {
                // 对外的 view/split 只会在根层调用；论文里这一层的逻辑元素
                // 类型是 `A`。
                unreachable!("top-level tree operation returned an internal branch")
            }
        }
    }
    fn clone_leaf(node: &NodeRef<A, R>, refs: &R) -> A {
        refs.with_node(node, |node| match &node.inner {
            NodeInner::Leaf(value) => value.clone(),
            NodeInner::Branch2 { .. } | NodeInner::Branch3 { .. } => {
                // 对外的 view/split 只会在根层调用；论文里这一层的逻辑元素
                // 类型是 `A`。
                unreachable!("top-level tree operation returned an internal branch")
            }
        })
    }
    fn into_digit(node: NodeRef<A, R>, refs: &R) -> Digit<NodeRef<A, R>> {
        match refs.into_node(node).inner {
            NodeInner::Branch2 { left, right } => Digit::Two([left, right]),
            NodeInner::Branch3 {
                left,
                middle,
                right,
            } => Digit::Three([left, middle, right]),
            // 只有来自中间树的递归结果会被展开成 Digit。论文里这些树的元素类型是
            // `Node v a`，不可能是 `a`。
            NodeInner::Leaf(_) => unreachable!("leaf node cannot be unlifted"),
        }
    }
    fn to_digit(node: &NodeRef<A, R>, refs: &R) -> Digit<NodeRef<A, R>> {
        refs.with_node(node, |node| match &node.inner {
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
    fn lift<I>(iter: I, refs: &R) -> LiftNodeIter<A, R, I::IntoIter>
    where
        I: IntoIterator<Item = NodeRef<A, R>>,
    {
        LiftNodeIter::new(iter.into_iter(), refs.clone())
    }
}

impl<A, R> Measured for Node<A, R>
where
    A: Measured,
    R: RefFamily<A>,
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

fn node_measure<A, R>(refs: &R, node: &NodeRef<A, R>) -> A::Measure
where
    A: Measured,
    R: RefFamily<A>,
{
    refs.with_node(node, |node| node.measure())
}

fn tree_measure<A, R>(refs: &R, tree: &TreeRef<A, R>) -> A::Measure
where
    A: Measured,
    R: RefFamily<A>,
{
    refs.with_tree(tree, |tree| tree.measure())
}

fn digit_measure<A, R>(digit: &Digit<NodeRef<A, R>>, refs: &R) -> A::Measure
where
    A: Measured,
    R: RefFamily<A>,
{
    node_refs_measure(refs, digit.as_slice().iter())
}

fn node_refs_measure<'a, A, R>(
    refs: &R,
    mut nodes: impl Iterator<Item = &'a NodeRef<A, R>>,
) -> A::Measure
where
    A: Measured,
    R: RefFamily<A>,
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
) -> Option<(
    Option<Digit<NodeRef<A, R>>>,
    NodeRef<A, R>,
    Option<Digit<NodeRef<A, R>>>,
)>
where
    A: Measured,
    R: RefFamily<A>,
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

impl<A, R, I> LiftNodeIter<A, R, I>
where
    A: Measured,
    R: RefFamily<A>,
    I: Iterator<Item = NodeRef<A, R>>,
{
    fn new(mut iter: I, refs: R) -> Self {
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
        let ret = core::mem::replace(&mut self.buf[self.cursor as usize], next).unwrap();
        self.cursor = (self.cursor + 1) % 5;
        ret
    }
}

impl<A, R, I> Iterator for LiftNodeIter<A, R, I>
where
    A: Measured,
    R: RefFamily<A>,
    I: Iterator<Item = NodeRef<A, R>>,
{
    type Item = NodeRef<A, R>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.live {
            0 => None,
            2 | 4 => {
                let left = self.pop_buffered();
                let right = self.pop_buffered();
                Some(self.refs.alloc_node(Node::branch2(left, right, &self.refs)))
            }
            3 | 5 => {
                let left = self.pop_buffered();
                let middle = self.pop_buffered();
                let right = self.pop_buffered();
                Some(
                    self.refs
                        .alloc_node(Node::branch3(left, middle, right, &self.refs)),
                )
            }
            // Node::lift 对应论文里的 `nodes` 辅助函数。它只在 concat 时处理
            // 左侧后缀 ++ 中间迭代器 ++ 右侧前缀；两侧 digit 都非空，所以输入
            // 长度至少为二。5 槽前瞻缓冲会把流分组成 Node2/Node3，不会留下
            // 单个尾元素。
            _ => unreachable!("cannot lift one remaining node"),
        }
    }
}
