use super::semigroup::*;

/// `empty().merge(&a) == a.merge(&empty()) == a`
pub trait Monoid
where
    Self: Semigroup,
{
    fn empty() -> Self;
}

impl<A: Monoid, B: Monoid> Monoid for (A, B) {
    #[inline]
    fn empty() -> Self {
        (A::empty(), B::empty())
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
}
impl<T: Semigroup> Monoid for Option<T> {
    #[inline]
    fn empty() -> Self {
        None
    }
}

#[derive(Clone, Copy)]
pub struct Size(pub usize);
impl Semigroup for Size {
    #[inline]
    fn merge(self, other: Self) -> Self {
        Size(self.0 + other.0)
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
}
macro_rules! impl_num_monoid {
    [$($t:ty),*] => {
        $(
        impl_monoid!(Sum<$t>, Sum(0 as $t));
        impl_monoid!(Product<$t>, Product(1 as $t));
        )*
    };
}

impl_num_monoid![u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, f32, f64];
impl_monoid!(Size, Size(0));
impl<T: Ord> Monoid for Min<T> {
    #[inline]
    fn empty() -> Self {
        Min::Inf
    }
}
impl<T: Ord> Monoid for Max<T> {
    #[inline]
    fn empty() -> Self {
        Max::NegInf
    }
}
