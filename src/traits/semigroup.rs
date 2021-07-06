use std::ops::{Add, Mul};

/// `a.merge(&b.merge(&c)) == a.merge(&b).merge(&c)`
pub trait Semigroup {
    fn merge(self, other: Self) -> Self;
}

impl<A: Semigroup, B: Semigroup> Semigroup for (A, B) {
    fn merge(self, (oa, ob): Self) -> Self {
        let (a, b) = self;
        (A::merge(a, oa), B::merge(b, ob))
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
