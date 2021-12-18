use super::super::traits::*;

use std::{
    cmp::{max, min},
    ops::Range,
    rc::Rc,
};

/// `m1.apply(a.merge(&b)) == m1.apply(a).merge(&m1.apply(b))`
pub trait Applier<V: Semigroup> {
    fn apply(&self, to: V) -> V;
}

impl<A: Semigroup, B: Semigroup, MA: Applier<A>, MB: Applier<B>> Applier<(A, B)> for (MA, MB) {
    #[inline]
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

#[derive(Debug)]
pub enum SegTree<V, M> {
    Empty,
    Unit(V),
    Branch {
        size: usize,
        modifier: M,
        value: V,
        left: Rc<SegTree<V, M>>,
        right: Rc<SegTree<V, M>>,
    },
}

impl<V, M> SegTree<V, M> {
    pub fn size(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Unit(_) => 1,
            Self::Branch { size, .. } => size.to_owned(),
        }
    }
}

impl<V: Monoid + Clone, M> SegTree<V, M> {
    fn all(&self) -> V {
        match self {
            Self::Empty => V::empty(),
            Self::Unit(v) => v.clone(),
            Self::Branch { value, .. } => value.clone(),
        }
    }
}

impl<V: Clone + Semigroup, M: Clone + Semigroup + Applier<V>> SegTree<V, M> {
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
                value: m.apply(value.clone()),
                modifier: M::merge(m, modifier.clone()),
                left: left.clone(),
                right: right.clone(),
            },
        }
    }
}

impl<V: Monoid + Clone, M: Monoid> SegTree<V, M> {
    fn build_inner<F: Fn(usize) -> V + Clone>(offset: usize, len: usize, init: F) -> Self {
        match len {
            0 => Self::Empty,
            1 => Self::Unit(init(offset)),
            len => {
                let mid = len / 2;
                let (l, r) = (
                    Self::build_inner(offset, mid, init.clone()),
                    Self::build_inner(offset + mid, len - mid, init),
                );

                Self::Branch {
                    size: len,
                    modifier: M::empty(),
                    value: V::merge(l.all(), r.all()),
                    left: Rc::new(l),
                    right: Rc::new(r),
                }
            }
        }
    }
}

impl<V: Monoid + Clone, M: Applier<V> + Monoid + Clone> SegTree<V, M> {
    pub fn build<F: Fn(usize) -> V + Clone>(len: usize, init: F) -> Self {
        Self::build_inner(0, len, init)
    }

    pub fn query(&self, range: Range<usize>) -> V {
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
                        left.query(range)
                    } else if mid <= range.start {
                        right.query(range.start - mid..range.end - mid)
                    } else {
                        V::merge(
                            left.query(range.start..mid),
                            right.query(0..range.end - mid),
                        )
                    })
                }
            }
        }
    }

    /// Apply a modifier to a SegTree
    ///
    /// # Arguments
    ///
    /// * `l` - The lower bound of the range (included)
    /// * `r` - The upper bound of the range (included)
    /// * `m` - The modifier to apply
    pub fn apply(&self, range: Range<usize>, m: M) -> Self {
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
                        value: m.apply(value.clone()),
                        modifier: M::merge(m, modifier.clone()),
                        left: left.clone(),
                        right: right.clone(),
                    }
                } else {
                    let mid = size / 2;
                    // push down
                    // to ensure the top modifier is the newest one.
                    let (left, right) = (
                        left.apply_all(modifier.clone()),
                        right.apply_all(modifier.clone()),
                    );

                    let (new_left, new_right) = (
                        Rc::new(if range.start < mid {
                            left.apply(range.start..min(range.end, mid), m.clone())
                        } else {
                            left
                        }),
                        Rc::new(if mid < range.end {
                            right.apply(max(range.start, mid) - mid..range.end - mid, m)
                        } else {
                            right
                        }),
                    );

                    Self::Branch {
                        size: *size,
                        modifier: M::empty(),
                        value: V::merge(new_left.all(), new_right.all()),
                        left: new_left,
                        right: new_right,
                    }
                }
            }
        }
    }
}
