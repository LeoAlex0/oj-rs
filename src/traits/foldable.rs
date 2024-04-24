pub trait Foldable {
    type Item;

    fn fold_left<T, F: Clone + Fn(T, &Self::Item) -> T>(&self, init: T, func: F) -> T;
    fn fold_right<T, F: Clone + Fn(&Self::Item, T) -> T>(&self, func: F, init: T) -> T;

    fn fold_map<T: crate::traits::monoid::Monoid, F: Clone + Fn(&Self::Item) -> T>(&self, func: F) -> T {
        self.fold_left(T::empty(), |t, i| t.merge(func(i)))
    }
}
