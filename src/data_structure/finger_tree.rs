use crate::traits::{monoid::Size, Monoid, Semigroup};

// Traits

pub trait Measured: Clone {
    type To: Monoid + Clone;
    fn measure(&self) -> Self::To;
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Value<T>(pub T);
impl<T: Clone> Measured for Value<T> {
    type To = Size;

    fn measure(&self) -> Self::To {
        Size(1)
    }
}

pub trait Ref<A>: AsRef<A> + std::ops::Deref<Target = A> + Clone {
    fn new(a: A) -> Self;
}

pub trait TreeRef<V: Measured>: Sized + Clone {
    type NodeRef: Ref<Node<V, Self>> + Measured<To = V::To>;
    type TreeRef: Ref<FingerTree<V, Self>> + Measured<To = V::To>;
}

impl<A: Measured> TreeRef<A> for RcRef {
    type NodeRef = std::rc::Rc<Node<A, Self>>;
    type TreeRef = std::rc::Rc<FingerTree<A, Self>>;
}
impl<A: Measured> TreeRef<A> for ArcRef {
    type NodeRef = std::sync::Arc<Node<A, Self>>;
    type TreeRef = std::sync::Arc<FingerTree<A, Self>>;
}

impl<A> Ref<A> for std::rc::Rc<A> {
    #[inline]
    fn new(a: A) -> Self {
        std::rc::Rc::new(a)
    }
}

impl<A: Measured> Measured for std::rc::Rc<A> {
    type To = A::To;

    fn measure(&self) -> Self::To {
        self.as_ref().measure()
    }
}

impl<A> Ref<A> for std::sync::Arc<A> {
    #[inline]
    fn new(a: A) -> Self {
        std::sync::Arc::new(a)
    }
}

impl<A: Measured> Measured for std::sync::Arc<A> {
    type To = A::To;

    fn measure(&self) -> Self::To {
        self.as_ref().measure()
    }
}

pub trait PersistMonoidIndexDeque<A: Measured>: Measured {
    fn new() -> Self;
    fn split<F: Fn(&A::To) -> bool>(&self, pred: F) -> Option<(Self, A, Self)>;
    fn concat(&self, other: &Self) -> Self;
    fn push_l(a: A, deq: &Self) -> Self;
    fn push_r(deq: &Self, a: A) -> Self;
    fn view_l(&self) -> Option<(A, Self)>;
    fn view_r(&self) -> Option<(Self, A)>;
}

//-------------------
// Struct & Enums
//-------------------
#[derive(Clone)]
pub struct RcRef;
#[derive(Clone)]
pub struct ArcRef;

#[derive(Clone)]
enum Digit<A> {
    One([A; 1]),
    Two([A; 2]),
    Three([A; 3]),
    Four([A; 4]),
}
struct DigitIter<A>(Option<Digit<A>>);

#[derive(Clone)]
pub struct Node<A: Measured, R: TreeRef<A>>(NodeInner<A, R>);

impl<A: Measured, R: TreeRef<A>> Measured for Node<A, R> {
    type To = A::To;

    fn measure(&self) -> Self::To {
        self.0.measure()
    }
}

#[derive(Clone)]
enum NodeInner<A, R>
where
    R: TreeRef<A>,
    A: Measured,
{
    Leaf(A),
    Node2 {
        measure: A::To,
        left: R::NodeRef,
        right: R::NodeRef,
    },
    Node3 {
        measure: A::To,
        left: R::NodeRef,
        middle: R::NodeRef,
        right: R::NodeRef,
    },
}
struct LiftNodeIter<R: TreeRef<A>, A: Measured, I: Iterator<Item = R::NodeRef>> {
    buff: [Option<R::NodeRef>; 5],
    left: u8,
    index: u8,
    iter: I,
}

#[derive(Clone)]
enum FingerTreeInner<A, R = RcRef>
where
    R: TreeRef<A>,
    A: Measured,
{
    Empty,
    Unit(R::NodeRef),
    Deep {
        measure: A::To,
        prefix: Digit<R::NodeRef>,
        deeper: R::TreeRef,
        suffix: Digit<R::NodeRef>,
    },
}

#[derive(Clone)]
pub struct FingerTree<A: Measured, R: TreeRef<A> = RcRef>(FingerTreeInner<A, R>);

impl<A: Measured, R: TreeRef<A>> std::iter::FromIterator<A> for FingerTree<A, R> {
    #[inline]
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        FingerTree(FingerTreeInner::push_many_r(
            FingerTreeInner::empty(),
            &mut iter
                .into_iter()
                .map(|x| R::NodeRef::new(Node(NodeInner::leaf(x)))),
        ))
    }
}

impl<A: Measured, R: TreeRef<A>> PersistMonoidIndexDeque<A> for FingerTree<A, R> {
    fn new() -> Self {
        FingerTree(FingerTreeInner::empty())
    }

    fn split<F: Fn(&A::To) -> bool>(&self, pred: F) -> Option<(Self, A, Self)> {
        self.0
            .clone()
            .split_offset(A::To::empty(), &pred)
            .map_or(None, |(front, mid, back)| match &*mid {
                Node(NodeInner::Leaf(a)) => Some((FingerTree(front), a.clone(), FingerTree(back))),
                _ => panic!("not the shallowest tree layer"),
            })
    }

    fn concat(&self, other: &Self) -> Self {
        FingerTree(FingerTreeInner::concat(self.0.clone(), other.0.clone()))
    }

    fn push_l(a: A, deq: &Self) -> Self {
        FingerTree(FingerTreeInner::push_l(
            R::NodeRef::new(Node(NodeInner::leaf(a))),
            deq.0.clone(),
        ))
    }

    fn push_r(deq: &Self, a: A) -> Self {
        FingerTree(FingerTreeInner::push_r(
            deq.0.clone(),
            R::NodeRef::new(Node(NodeInner::leaf(a))),
        ))
    }

    fn view_l(&self) -> Option<(A, Self)> {
        self.0.clone().view_l().map_or(None, |(a, tree)| match &*a {
            Node(NodeInner::Leaf(a)) => Some((a.clone(), FingerTree(tree))),
            _ => panic!("not the shallowest tree layer"),
        })
    }

    fn view_r(&self) -> Option<(Self, A)> {
        self.0.clone().view_r().map_or(None, |(tree, a)| match &*a {
            Node(NodeInner::Leaf(a)) => Some((FingerTree(tree), a.clone())),
            _ => panic!("not the shallowest tree layer"),
        })
    }
}

impl<A: Measured, R: TreeRef<A>> Measured for FingerTree<A, R> {
    type To = A::To;
    fn measure(&self) -> Self::To {
        self.0.measure()
    }
}

//-------------------------
// Impl
//-------------------------

impl<A> Measured for Digit<A>
where
    A: Measured,
{
    type To = A::To;
    fn measure(&self) -> Self::To {
        match self {
            Self::One([a]) => a.measure(),
            Self::Two([a, b]) => A::To::merge(a.measure(), b.measure()),
            Self::Three([a, b, c]) => a.measure().merge(b.measure()).merge(c.measure()),
            Self::Four([a, b, c, d]) => A::To::merge(
                A::To::merge(a.measure(), b.measure()),
                A::To::merge(c.measure(), d.measure()),
            ),
        }
    }
}

impl<A, R> Measured for NodeInner<A, R>
where
    R: TreeRef<A>,
    A: Measured,
{
    type To = A::To;

    fn measure(&self) -> Self::To {
        match self {
            Self::Leaf(refd) => refd.measure(),
            NodeInner::Node2 { measure, .. } => measure.clone(),
            NodeInner::Node3 { measure, .. } => measure.clone(),
        }
    }
}

impl<A, R> Measured for FingerTreeInner<A, R>
where
    R: TreeRef<A>,
    A: Measured,
{
    type To = A::To;

    fn measure(&self) -> Self::To {
        match self {
            Self::Empty => A::To::empty(),
            Self::Unit(v) => v.measure(),
            Self::Deep { measure, .. } => measure.clone(),
        }
    }
}

impl<A> AsRef<[A]> for Digit<A> {
    fn as_ref(&self) -> &[A] {
        match self {
            Self::One(a) => a,
            Self::Two(a) => a,
            Self::Three(a) => a,
            Self::Four(a) => a,
        }
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

impl<A> IntoIterator for Digit<A> {
    type Item = A;

    type IntoIter = DigitIter<A>;

    fn into_iter(self) -> Self::IntoIter {
        DigitIter(Some(self))
    }
}

impl<A> std::iter::FromIterator<A> for Digit<A> {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let mut ret = None;
        for a in iter {
            ret = match ret {
                None => Some(Self::One([a])),
                Some(digit) => Some(Self::push_right(digit, a)),
            }
        }
        ret.unwrap()
    }
}

impl<A> Digit<A> {
    fn push_left(a: A, digit: Self) -> Self {
        match digit {
            Self::One([b]) => Self::Two([a, b]),
            Self::Two([b, c]) => Self::Three([a, b, c]),
            Self::Three([b, c, d]) => Self::Four([a, b, c, d]),
            _ => panic!("cannot push element to a 4 element digit"),
        }
    }
    fn option_push_left(a: A, digit: Option<Self>) -> Self {
        match digit {
            None => Digit::One([a]),
            Some(digit) => Self::push_left(a, digit),
        }
    }

    fn push_right(digit: Self, a: A) -> Self {
        match digit {
            Self::One([b]) => Self::Two([b, a]),
            Self::Two([c, b]) => Self::Three([c, b, a]),
            Self::Three([d, c, b]) => Self::Four([d, c, b, a]),
            _ => panic!("cannot push element to a 4 element digit"),
        }
    }

    fn view_left(self) -> (A, Option<Self>) {
        match self {
            Self::One([a]) => (a, None),
            Self::Two([a, b]) => (a, Some(Self::One([b]))),
            Self::Three([a, b, c]) => (a, Some(Self::Two([b, c]))),
            Self::Four([a, b, c, d]) => (a, Some(Self::Three([b, c, d]))),
        }
    }
    fn view_right(self) -> (Option<Self>, A) {
        match self {
            Self::One([a]) => (None, a),
            Self::Two([b, a]) => (Some(Self::One([b])), a),
            Self::Three([c, b, a]) => (Some(Self::Two([c, b])), a),
            Self::Four([d, c, b, a]) => (Some(Self::Three([d, c, b])), a),
        }
    }
}

impl<A: Measured> Digit<A> {
    fn split_offset<F: Fn(&A::To) -> bool>(
        self,
        offset: A::To,
        pred: &F,
    ) -> (Option<Digit<A>>, A, Option<Digit<A>>) {
        let (cur, tail) = self.view_left();
        let cur_measure = offset.merge(cur.measure());
        if pred(&cur_measure) || tail.is_none() {
            (None, cur, tail)
        } else {
            let (ofront, a, otail) = tail.unwrap().split_offset(cur_measure, pred);
            (Some(Digit::option_push_left(cur, ofront)), a, otail)
        }
    }
}

impl<A: Measured, R: TreeRef<A>> NodeInner<A, R> {
    fn leaf(val: A) -> Self {
        Self::Leaf(val)
    }

    fn node2(left: R::NodeRef, right: R::NodeRef) -> Self {
        let measure = left.measure().merge(right.measure());
        Self::Node2 {
            measure,
            left,
            right,
        }
    }

    fn node3(left: R::NodeRef, middle: R::NodeRef, right: R::NodeRef) -> Self {
        let measure = left
            .measure()
            .merge(middle.measure())
            .merge(right.measure());
        Self::Node3 {
            measure,
            left,
            middle,
            right,
        }
    }

    fn lift<I: IntoIterator<Item = R::NodeRef>>(iter: I) -> LiftNodeIter<R, A, I::IntoIter> {
        LiftNodeIter::new(iter.into_iter())
    }

    fn unlift_digit(self) -> Digit<R::NodeRef> {
        match self {
            Self::Node2 { left, right, .. } => Digit::Two([left, right]),
            Self::Node3 {
                left,
                middle,
                right,
                ..
            } => Digit::Three([left, middle, right]),
            _ => panic!("Leaf node cannot lift to digit"),
        }
    }
}

impl<R, A, I> LiftNodeIter<R, A, I>
where
    R: TreeRef<A>,
    A: Measured,
    I: Iterator<Item = R::NodeRef>,
{
    fn new(mut iter: I) -> Self {
        let buff = [
            iter.next(),
            iter.next(),
            iter.next(),
            iter.next(),
            iter.next(),
        ];
        let left = buff.iter().filter(|e| e.is_some()).count();
        Self {
            buff,
            iter,
            left: left as u8,
            index: 0,
        }
    }

    fn next_subtree(&mut self) -> <Self as Iterator>::Item {
        let new = self.iter.next();
        if new.is_none() {
            self.left -= 1
        }
        let ret = core::mem::replace(&mut self.buff[self.index as usize], new).unwrap();
        self.index = (self.index + 1) % 5;
        ret
    }
}

impl<R, A, I> Iterator for LiftNodeIter<R, A, I>
where
    R: TreeRef<A>,
    A: Measured,
    I: Iterator<Item = R::NodeRef>,
{
    type Item = R::NodeRef;

    fn next(&mut self) -> Option<Self::Item> {
        match self.left {
            0 => None,
            2 | 4 => {
                let left = self.next_subtree();
                let right = self.next_subtree();
                Some(R::NodeRef::new(Node(NodeInner::node2(left, right))))
            }
            3 | 5 => {
                let left = self.next_subtree();
                let middle = self.next_subtree();
                let right = self.next_subtree();
                Some(R::NodeRef::new(Node(NodeInner::node3(left, middle, right))))
            }
            _ => panic!("cannot lift node with only 1 subtree"),
        }
    }
}

impl<A, R> FingerTreeInner<A, R>
where
    R: TreeRef<A>,
    A: Measured,
{
    #[inline]
    fn empty() -> Self {
        Self::Empty
    }

    #[inline]
    fn single(unit: R::NodeRef) -> Self {
        Self::Unit(unit)
    }

    #[inline]
    fn deep(prefix: Digit<R::NodeRef>, deeper: R::TreeRef, suffix: Digit<R::NodeRef>) -> Self {
        let measure = prefix
            .measure()
            .merge(deeper.measure())
            .merge(suffix.measure());
        Self::Deep {
            measure,
            prefix,
            deeper: deeper.clone(),
            suffix,
        }
    }

    fn push_l(a: R::NodeRef, tree: Self) -> Self {
        match tree {
            Self::Empty => Self::single(a),
            Self::Unit(b) => Self::deep(
                Digit::One([a]),
                R::TreeRef::new(FingerTree(Self::empty())),
                Digit::One([b]),
            ),
            Self::Deep {
                prefix: Digit::Four([b, c, d, e]),
                deeper,
                suffix,
                ..
            } => Self::deep(
                Digit::Two([a, b]),
                R::TreeRef::new(FingerTree(Self::push_l(
                    R::NodeRef::new(Node(NodeInner::node3(c, d, e))),
                    deeper.0.clone(),
                ))),
                suffix,
            ),
            Self::Deep {
                prefix,
                deeper,
                suffix,
                ..
            } => Self::deep(Digit::push_left(a, prefix), deeper.clone(), suffix),
        }
    }

    fn push_many_l(iter: &mut dyn Iterator<Item = R::NodeRef>, tree: Self) -> Self {
        match iter.next() {
            None => tree,
            Some(a) => Self::push_l(a, Self::push_many_l(iter, tree)),
        }
    }

    fn push_r(tree: Self, a: R::NodeRef) -> Self {
        match tree {
            Self::Empty => Self::single(a),
            Self::Unit(b) => Self::deep(
                Digit::One([b]),
                R::TreeRef::new(FingerTree(Self::empty())),
                Digit::One([a]),
            ),
            Self::Deep {
                prefix,
                deeper,
                suffix: Digit::Four([e, d, c, b]),
                ..
            } => Self::deep(
                prefix,
                R::TreeRef::new(FingerTree(Self::push_r(
                    deeper.as_ref().clone().0,
                    R::NodeRef::new(Node(NodeInner::node3(e, d, c))),
                ))),
                Digit::Two([b, a]),
            ),
            Self::Deep {
                prefix,
                deeper,
                suffix,
                ..
            } => Self::deep(prefix, deeper.clone(), Digit::push_right(suffix, a)),
        }
    }

    fn push_many_r(mut tree: Self, iter: &mut dyn Iterator<Item = R::NodeRef>) -> Self {
        for a in iter {
            tree = Self::push_r(tree, a);
        }
        tree
    }

    #[inline]
    fn to_tree(iter: &mut dyn Iterator<Item = R::NodeRef>) -> Self {
        Self::push_many_r(Self::empty(), iter)
    }

    fn deep_l(
        prefix: Option<Digit<R::NodeRef>>,
        deeper: R::TreeRef,
        suffix: Digit<R::NodeRef>,
    ) -> Self {
        match prefix {
            None => match deeper.0.clone().view_l() {
                Some((prefix, tree)) => Self::deep(
                    prefix.0.clone().unlift_digit(),
                    R::TreeRef::new(FingerTree(tree)),
                    suffix,
                ),
                None => Self::to_tree(&mut suffix.into_iter()),
            },
            Some(prefix) => Self::deep(prefix, deeper, suffix),
        }
    }

    fn view_l(self) -> Option<(R::NodeRef, Self)> {
        match self {
            Self::Empty => None,
            Self::Unit(a) => Some((a, Self::empty())),
            Self::Deep {
                prefix,
                deeper,
                suffix,
                ..
            } => {
                let (head, tail) = prefix.view_left();
                Some((head, Self::deep_l(tail, deeper.clone(), suffix)))
            }
        }
    }

    fn deep_r(
        prefix: Digit<R::NodeRef>,
        deeper: R::TreeRef,
        suffix: Option<Digit<R::NodeRef>>,
    ) -> Self {
        match suffix {
            None => match deeper.0.clone().view_r() {
                Some((tree, suffix)) => Self::deep(
                    prefix,
                    R::TreeRef::new(FingerTree(tree)),
                    suffix.0.clone().unlift_digit(),
                ),
                None => Self::to_tree(&mut prefix.into_iter()),
            },
            Some(suffix) => Self::deep(prefix, deeper, suffix),
        }
    }

    fn view_r(self) -> Option<(Self, R::NodeRef)> {
        match self {
            Self::Empty => None,
            Self::Unit(a) => Some((Self::empty(), a)),
            Self::Deep {
                prefix,
                deeper,
                suffix,
                ..
            } => {
                let (init, last) = suffix.view_right();
                Some((Self::deep_r(prefix, deeper.clone(), init), last))
            }
        }
    }

    fn split_offset<F: Fn(&A::To) -> bool>(
        self,
        offset: A::To,
        pred: &F,
    ) -> Option<(Self, R::NodeRef, Self)> {
        match self {
            Self::Empty => None,
            Self::Unit(a) => Some((Self::Empty, a, Self::Empty)),
            Self::Deep {
                prefix,
                deeper,
                suffix,
                ..
            } => {
                let with_prefix = offset.clone().merge(prefix.measure());
                if pred(&with_prefix) {
                    let (obefore, a, oafter) = prefix.split_offset(offset, pred);
                    return Some((
                        obefore
                            .map_or(Self::empty(), |digit| Self::to_tree(&mut digit.into_iter())),
                        a,
                        Self::deep_l(oafter, deeper, suffix),
                    ));
                }
                let with_deeper = with_prefix.clone().merge(deeper.measure());
                if pred(&with_deeper) {
                    let (before, node, after) = deeper
                        .0
                        .clone()
                        .split_offset(with_prefix.clone(), pred)
                        .unwrap();
                    let (opr, a, osf) = node
                        .0
                        .clone()
                        .unlift_digit()
                        .split_offset(with_prefix.merge(before.measure()), pred);
                    return Some((
                        Self::deep_r(prefix, R::TreeRef::new(FingerTree(before)), opr),
                        a,
                        Self::deep_l(osf, R::TreeRef::new(FingerTree(after)), suffix),
                    ));
                }
                let (obefore, a, oafter) = suffix.split_offset(with_deeper, pred);
                Some((
                    Self::deep_r(prefix, deeper, obefore),
                    a,
                    oafter.map_or(Self::empty(), |digit| Self::to_tree(&mut digit.into_iter())),
                ))
            }
        }
    }

    fn concat(front: Self, back: Self) -> Self {
        Self::concat_3_way(front, &mut std::iter::empty(), back)
    }

    fn concat_3_way(front: Self, mid: &mut dyn Iterator<Item = R::NodeRef>, back: Self) -> Self {
        match (front, back) {
            (Self::Empty, back) => Self::push_many_l(mid, back),
            (front, Self::Empty) => Self::push_many_r(front, mid),
            (Self::Unit(a), back) => Self::push_l(a, Self::push_many_l(mid, back)),
            (front, Self::Unit(a)) => Self::push_r(Self::push_many_r(front, mid), a),
            (
                Self::Deep {
                    prefix: pr1,
                    deeper: m1,
                    suffix: sf1,
                    ..
                },
                Self::Deep {
                    prefix: pr2,
                    deeper: m2,
                    suffix: sf2,
                    ..
                },
            ) => Self::deep(
                pr1,
                R::TreeRef::new(FingerTree(Self::concat_3_way(
                    m1.0.clone(),
                    &mut NodeInner::<A, R>::lift(sf1.into_iter().chain(mid).chain(pr2)),
                    m2.0.clone(),
                ))),
                sf2,
            ),
        }
    }
}
