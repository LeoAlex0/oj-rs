use std::{
    cmp::{max, min},
    ops::Range,
    rc::Rc,
};

/// `a.merge(&b.merge(&c)) == a.merge(&b).merge(&c)`
pub trait Semigroup {
    fn merge(self, other: &Self) -> Self;
}

impl<A: Semigroup, B: Semigroup> Semigroup for (A, B) {
    fn merge(self, other: &Self) -> Self {
        (A::merge(self.0, &other.0), B::merge(self.1, &other.1))
    }
}

/// `empty().merge(&a) == a.merge(&empty()) == a`
pub trait Monoid
where
    Self: Semigroup,
{
    fn empty() -> Self;
}

impl<A: Monoid, B: Monoid> Monoid for (A, B) {
    fn empty() -> Self {
        (A::empty(), B::empty())
    }
}

pub trait Applier<V> {
    fn apply(&self, to: V) -> V;
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

impl<V: Clone, M: Semigroup + Applier<V>> SegTree<V, M> {
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
                modifier: m.merge(modifier),
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
                    value: l.all().merge(&r.all()),
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
                if range.start <= 0 && *size <= range.end {
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
                            &right.query(0..range.end - mid),
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
                if range.start <= 0 && *size <= range.end {
                    Self::Branch {
                        size: *size,
                        value: m.apply(value.clone()),
                        modifier: m.merge(modifier),
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
                        value: V::merge(new_left.all(), &new_right.all()),
                        left: new_left,
                        right: new_right,
                    }
                }
            }
        }
    }
}
