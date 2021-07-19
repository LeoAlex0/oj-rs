use std::{cmp::max, rc::Rc, usize};

fn main() {}

mod TreeAgent {
    #[derive(Default)]
    struct NodeRecord {
        ord: usize,
        subtree_size: usize,
        group_head: usize,
    }

    struct TreeAgent {
        len: usize,
        nodes: Vec<NodeRecord>,
    }
}

trait Monoid {
    fn empty() -> Self;
    fn merge(self, another: &Self) -> Self;
}

trait Commutative {}

trait FMonoid<V>
where
    Self: Monoid + Commutative,
    V: Monoid,
{
    fn apply(&self, val: V) -> V;
}

#[derive(Debug)]
enum SegTree<V, M>
where
    V: Monoid,
    M: FMonoid<V>,
{
    Branch {
        transfer: M,
        left: Rc<SegTree<V, M>>,
        right: Rc<SegTree<V, M>>,
        value: V,
        size: usize,
    },
    Unit(V),
}

impl<V, M> SegTree<V, M>
where
    V: Monoid,
    M: FMonoid<V>,
{
    pub(crate) fn build<F: Clone + Fn(usize) -> V>(init: F, len: usize) -> Self {
        Self::build_offset(init, len, 0)
    }

    pub(crate) fn size(&self) -> usize {
        match &self {
            Self::Unit(_) => 1,
            Self::Branch { size, .. } => size.to_owned(),
        }
    }

    pub(crate) fn query(&self, range: (usize, usize)) -> V {
        let mut ret = V::empty();
        match self {
            Self::Unit(v) => ret.merge(v),
            Self::Branch {
                left,
                right,
                size,
                value,
                transfer,
                ..
            } => {
                if range.0 == 0 && size.to_owned() + 1 <= range.1 {
                    ret.merge(value)
                } else {
                    let mid = left.size();
                    if range.0 < mid {
                        ret = ret.merge(&left.query(range));
                    }
                    if mid <= range.1 {
                        ret = ret.merge(&right.query((max(range.0, mid) - mid, range.1 - mid)));
                    }
                    transfer.apply(ret)
                }
            }
        }
    }

    pub(crate) fn apply(&self, trans: &M, range: (usize, usize)) -> Self {
        match self {
            Self::Unit(v) => Self::Unit(trans.apply(V::empty().merge(v))),
            Self::Branch {
                transfer,
                left,
                right,
                value,
                size,
            } => {
                let mid = left.size();
                if range.0 == 0 && size + 1 <= range.1 {
                    // full node
                    Self::Branch {
                        transfer: M::empty().merge(trans).merge(transfer),
                        value: trans.apply(V::empty().merge(value)),
                        left: left.clone(),
                        right: right.clone(),
                        size: size.to_owned(),
                    }
                } else if range.0 < mid && range.1 < mid {
                    // full left
                    let new_left = left.apply(trans, range);
                    Self::Branch {
                        transfer: M::empty().merge(transfer),
                        value: transfer.apply(V::empty().merge(new_left.all()).merge(right.all())),
                        left: Rc::new(new_left),
                        right: right.clone(),
                        size: size.to_owned(),
                    }
                } else if mid <= range.0 && mid <= range.1 {
                    // full right
                    let new_right = right.apply(trans, (range.0 - mid, range.1 - mid));
                    Self::Branch {
                        transfer: M::empty().merge(transfer),
                        value: transfer.apply(V::empty().merge(left.all()).merge(new_right.all())),
                        left: left.clone(),
                        right: Rc::new(new_right),
                        size: size.to_owned(),
                    }
                } else {
                    // LR Mix
                    let new_left = left.apply(trans, range);
                    let new_right = right.apply(trans, (range.0 - mid, range.1 - mid));
                    Self::Branch {
                        transfer: M::empty().merge(transfer),
                        value: transfer
                            .apply(V::empty().merge(new_left.all()).merge(new_right.all())),
                        left: Rc::new(new_left),
                        right: Rc::new(new_right),
                        size: size.to_owned(),
                    }
                }
            }
        }
    }

    pub(crate) fn all(&self) -> &V {
        match &self {
            Self::Unit(v) => v,
            Self::Branch { value, .. } => value,
        }
    }

    fn build_offset<F: Clone + Fn(usize) -> V>(init: F, len: usize, offset: usize) -> Self {
        if len == 1 {
            Self::Unit(init(offset))
        } else {
            let r_len = len / 2;
            let l_len = len - r_len;

            let left = Self::build_offset(init.clone(), l_len, offset);
            let right = Self::build_offset(init, r_len, offset + l_len);
            let value = V::empty().merge(left.all()).merge(right.all());

            Self::Branch {
                transfer: M::empty(),
                left: Rc::new(left),
                right: Rc::new(right),
                value,
                size: len,
            }
        }
    }
}
