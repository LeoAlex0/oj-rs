use std::{rc::Rc, sync::Arc};

use crate::traits::{Monoid, Semigroup};

// Traits

pub trait Measured: Clone {
    type To: Monoid + Clone;
    fn measure(&self) -> Self::To;
}
pub trait Ref<A>: Clone {
    // Ref<A>::Reffed -> SomeRef<A>
    type Reffed: AsRef<A> + Clone;
}

//-------------------
// Struct & Enums
//-------------------
#[derive(Clone)]
pub struct RcRef;
#[derive(Clone)]
pub struct ArcRef;
#[derive(Clone)]
pub struct NormalRef;
impl<A> Ref<A> for RcRef {
    type Reffed = Rc<A>;
}
impl<A> Ref<A> for ArcRef {
    type Reffed = Arc<A>;
}

#[derive(Clone)]
enum Digit<A> {
    One([A; 1]),
    Two([A; 2]),
    Three([A; 3]),
    Four([A; 4]),
}

#[derive(Clone)]
enum Node<R, A>
where
    R: Ref<A> + Ref<Node<R, A>>,
    A: Measured,
{
    Leaf(<R as Ref<A>>::Reffed),
    Node2 {
        measure: A::To,
        left: <R as Ref<Node<R, A>>>::Reffed,
        right: <R as Ref<Node<R, A>>>::Reffed,
    },
    Node3 {
        measure: A::To,
        left: <R as Ref<Node<R, A>>>::Reffed,
        middle: <R as Ref<Node<R, A>>>::Reffed,
        right: <R as Ref<Node<R, A>>>::Reffed,
    },
}

#[derive(Clone)]
enum FingerTree<R, A>
where
    R: Ref<A> + Ref<Node<R, A>> + Ref<FingerTree<R, A>>,
    A: Measured,
{
    Empty,
    Unit(<R as Ref<A>>::Reffed),
    Deep {
        measure: A::To,
        prefix: Digit<<R as Ref<Node<R, A>>>::Reffed>,
        deeper: <R as Ref<FingerTree<R, A>>>::Reffed,
        suffix: Digit<<R as Ref<Node<R, A>>>::Reffed>,
    },
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

impl<R, A> Measured for Node<R, A>
where
    R: Ref<A> + Ref<Node<R, A>>,
    A: Measured,
{
    type To = A::To;

    fn measure(&self) -> Self::To {
        match self {
            Self::Leaf(refd) => refd.as_ref().measure(),
            Node::Node2 { measure, .. } => measure.clone(),
            Node::Node3 { measure, .. } => measure.clone(),
        }
    }
}

impl<R, A> Measured for FingerTree<R, A>
where
    R: Ref<A> + Ref<Node<R, A>> + Ref<FingerTree<R, A>>,
    A: Measured,
{
    type To = A::To;

    fn measure(&self) -> Self::To {
        match self {
            Self::Empty => A::To::empty(),
            Self::Unit(v) => v.as_ref().measure(),
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
