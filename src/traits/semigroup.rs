use std::ops::{Add, Mul};

/// `a.merge(&b.merge(&c)) == a.merge(&b).merge(&c)`
pub trait Semigroup {
    fn merge(self, other: Self) -> Self;

    /// 右侧合并：`self = old_self.merge(other)`。
    ///
    /// 这是给热路径用的可选优化入口。默认实现保持和 `merge` 完全一致的语义；
    /// 大对象可以覆写它，避免为了更新左操作数而先 clone 一份旧值。
    #[inline]
    fn merge_assign(&mut self, other: &Self)
    where
        Self: Clone,
    {
        *self = self.clone().merge(other.clone());
    }

    /// 左侧合并：`self = other.merge(old_self)`。
    ///
    /// 非交换半群里方向很重要，线段树 lazy 标记合成就属于这种场景。把它单独
    /// 命名可以避免把“新标记在左还是在右”藏进调用点的 clone/move 细节里。
    #[inline]
    fn prepend_assign(&mut self, other: &Self)
    where
        Self: Clone,
    {
        *self = other.clone().merge(self.clone());
    }
}

impl<A: Semigroup, B: Semigroup> Semigroup for (A, B) {
    fn merge(self, (oa, ob): Self) -> Self {
        let (a, b) = self;
        (A::merge(a, oa), B::merge(b, ob))
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Identity<V>(pub V);

impl<V> Semigroup for Identity<V> {
    fn merge(self, _: Self) -> Self {
        self
    }

    #[inline]
    fn merge_assign(&mut self, _: &Self)
    where
        Self: Clone,
    {
    }

    #[inline]
    fn prepend_assign(&mut self, other: &Self)
    where
        Self: Clone,
    {
        *self = other.clone();
    }
}

macro_rules! impl_semigroup {
    ($t:ty,$k:path,$v:expr) => {
        impl<T: $k> Semigroup for $t {
            #[inline]
            fn merge(self, other: Self) -> Self {
                $v(self, other)
            }
        }
    };
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Sum<T: Add<Output = T>>(pub T);
impl<T: Add<Output = T>> Add for Sum<T> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Sum(self.0 + rhs.0)
    }
}
impl_semigroup!(Sum<T>, Add<Output = T>, <Sum<T> as Add>::add);

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Product<T: Mul<Output = T>>(pub T);
impl<T: Mul<Output = T>> Mul for Product<T> {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Product(self.0 * rhs.0)
    }
}
impl_semigroup!(Product<T>, Mul<Output = T>, <Product<T> as Mul>::mul);

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum Max<O: Ord> {
    NegInf,
    Has(O),
}
impl_semigroup!(Max<T>, Ord, <Max<T> as Ord>::max);

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum Min<O: Ord> {
    Has(O),
    Inf,
}
impl_semigroup!(Min<T>, Ord, <Min<T> as Ord>::min);
