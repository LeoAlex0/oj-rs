pub mod seg_tree;

pub mod group {
    pub use super::seg_tree::Monoid;
    pub use super::seg_tree::Semigroup;
}
