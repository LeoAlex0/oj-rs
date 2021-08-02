use core::panic;
use std::{ops::Add, rc::Rc};

use crate::traits::{Monoid, Semigroup};

pub trait Measured: Clone {
    type To;
    fn measure(&self) -> Self::To;
}

#[derive(Clone)]
enum NodeInner<A: Measured> {
    Leaf(A),
    Node2 {
        measure: A::To,
        left: Node<A>,
        right: Node<A>,
    },
    Node3 {
        measure: A::To,
        left: Node<A>,
        middle: Node<A>,
        right: Node<A>,
    },
}
type Node<A> = Rc<NodeInner<A>>;
impl<A: Measured> Measured for NodeInner<A>
where
    A::To: Semigroup + Clone,
{
    type To = A::To;

    fn measure(&self) -> Self::To {
        match self {
            NodeInner::Leaf(v) => v.measure(),
            NodeInner::Node2 { measure, .. } => measure.clone(),
            NodeInner::Node3 { measure, .. } => measure.clone(),
        }
    }
}
impl<A: Measured> NodeInner<A>
where
    A::To: Monoid,
{
    fn splitAt<F: Clone + Fn(A::To) -> bool>() -> (Option<NodeInner<A>>, A, Option<NodeInner<A>>) {
        todo!()
    }
}

#[derive(Clone)]
enum Digit<A> {
    One([A; 1]),
    Two([A; 2]),
    Three([A; 3]),
    Four([A; 4]),
}
impl<A> AsRef<[A]> for Digit<A> {
    fn as_ref(&self) -> &[A] {
        match self {
            Self::One(v) => v,
            Self::Two(v) => v,
            Self::Three(v) => v,
            Self::Four(v) => v,
        }
    }
}
impl<'a, A, R> Add<R> for &'a Digit<A>
where
    A: Clone,
    R: AsRef<[A]>,
{
    type Output = Digit<A>;

    fn add(self, rhs: R) -> Self::Output {
        match (self.as_ref(), rhs.as_ref()) {
            ([a], []) => Digit::One([a.clone()]),
            ([a], [b]) | ([a, b], []) => Digit::Two([a.clone(), b.clone()]),
            ([a], [b, c]) | ([a, b], [c]) | ([a, b, c], []) => {
                Digit::Three([a.clone(), b.clone(), c.clone()])
            }
            ([a], [b, c, d]) | ([a, b], [c, d]) | ([a, b, c], [d]) | ([a, b, c, d], []) => {
                Digit::Four([a.clone(), b.clone(), c.clone(), d.clone()])
            }
            (x, y) => panic!(
                "Too long, Digit only support max 4 element but actually {} + {} element",
                x.len(),
                y.len()
            ),
        }
    }
}
impl<'a, A> From<&'a [A]> for Digit<A>
where
    A: Clone,
{
    fn from(slice: &'a [A]) -> Digit<A> {
        match slice {
            [v0] => Digit::One([v0.clone()]),
            [v0, v1] => Digit::Two([v0.clone(), v1.clone()]),
            [v0, v1, v2] => Digit::Three([v0.clone(), v1.clone(), v2.clone()]),
            [v0, v1, v2, v3] => Digit::Four([v0.clone(), v1.clone(), v2.clone(), v3.clone()]),
            _ => panic!("immposible to create digit from of size: {}", slice.len()),
        }
    }
}
impl<A: Measured> Measured for Digit<A>
where
    A::To: Semigroup,
{
    type To = A::To;
    fn measure(&self) -> Self::To {
        match self {
            Digit::One([a]) => a.measure(),
            Digit::Two([a, b]) => A::To::merge(a.measure(), b.measure()),
            Digit::Three([a, b, c]) => a.measure().merge(b.measure()).merge(c.measure()),
            Digit::Four([a, b, c, d]) => A::To::merge(
                a.measure().merge(b.measure()),
                c.measure().merge(d.measure()),
            ),
        }
    }
}

#[derive(Clone)]
enum FingerTreeImpl<A: Measured> {
    Empty,
    Unit(A),
    Deep {
        measure: A::To,
        pr: Digit<Node<A>>,
        m: FingerTree<A>,
        sf: Digit<Node<A>>,
    },
}
#[derive(Clone)]
pub struct FingerTree<A: Measured>(Rc<FingerTreeImpl<A>>);

impl<A: Measured> Measured for FingerTreeImpl<A>
where
    A::To: Clone + Monoid,
{
    type To = A::To;

    fn measure(&self) -> Self::To {
        match self {
            Self::Empty => A::To::empty(),
            Self::Unit(a) => a.measure(),
            Self::Deep { measure, .. } => measure.clone(),
        }
    }
}
