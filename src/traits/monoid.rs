use super::semigroup::*;

/// `empty().merge(&a) == a.merge(&empty()) == a`
pub trait Monoid
where
    Self: Semigroup,
{
    fn empty() -> Self;

    /// 性能提示：返回 true 时必须保证 `self` 等价于 `empty()`。
    ///
    /// 默认返回 false，以便没有显式实现的类型仍然保持正确，只是不能跳过空操作。
    fn is_empty(&self) -> bool {
        false
    }
}

impl<A: Monoid, B: Monoid> Monoid for (A, B) {
    #[inline]
    fn empty() -> Self {
        (A::empty(), B::empty())
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.0.is_empty() && self.1.is_empty()
    }
}

impl<T: Semigroup> Semigroup for Option<T> {
    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (None, a) => a,
            (a, None) => a,
            (Some(a), Some(b)) => Some(T::merge(a, b)),
        }
    }

    #[inline]
    fn merge_assign(&mut self, other: &Self)
    where
        Self: Clone,
    {
        *self = match (self.take(), other.clone()) {
            (None, a) => a,
            (a, None) => a,
            (Some(a), Some(b)) => Some(T::merge(a, b)),
        };
    }

    #[inline]
    fn prepend_assign(&mut self, other: &Self)
    where
        Self: Clone,
    {
        *self = match (other.clone(), self.take()) {
            (None, a) => a,
            (a, None) => a,
            (Some(a), Some(b)) => Some(T::merge(a, b)),
        };
    }
}
impl<T: Semigroup> Monoid for Option<T> {
    #[inline]
    fn empty() -> Self {
        None
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.is_none()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Size(pub usize);
impl Semigroup for Size {
    #[inline]
    fn merge(self, other: Self) -> Self {
        Size(self.0 + other.0)
    }

    #[inline]
    fn merge_assign(&mut self, other: &Self) {
        self.0 += other.0;
    }

    #[inline]
    fn prepend_assign(&mut self, other: &Self) {
        self.merge_assign(other);
    }
}
impl Default for Size {
    #[inline]
    fn default() -> Self {
        Size(1)
    }
}

macro_rules! impl_monoid {
    ($t:ty, $v:expr) => {
        impl Monoid for $t {
            #[inline]
            fn empty() -> $t {
                $v
            }
        }
    };
    ($t:ty, $v:expr, $is_empty:expr) => {
        impl Monoid for $t {
            #[inline]
            fn empty() -> $t {
                $v
            }

            #[inline]
            fn is_empty(&self) -> bool {
                $is_empty(self)
            }
        }
    };
}
macro_rules! impl_num_monoid {
    [$($t:ty),*] => {
        $(
        impl_monoid!(Sum<$t>, Sum(0 as $t), |value: &Sum<$t>| value.0 == 0 as $t);
        impl_monoid!(Product<$t>, Product(1 as $t), |value: &Product<$t>| value.0 == 1 as $t);
        )*
    };
}

impl_num_monoid![u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, f32, f64];
impl_monoid!(Size, Size(0), |value: &Size| value.0 == 0);
impl<T: Ord> Monoid for Min<T> {
    #[inline]
    fn empty() -> Self {
        Min::Inf
    }

    #[inline]
    fn is_empty(&self) -> bool {
        matches!(self, Min::Inf)
    }
}
impl<T: Ord> Monoid for Max<T> {
    #[inline]
    fn empty() -> Self {
        Max::NegInf
    }

    #[inline]
    fn is_empty(&self) -> bool {
        matches!(self, Max::NegInf)
    }
}
