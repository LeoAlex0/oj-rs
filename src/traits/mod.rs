pub mod foldable;
pub mod monoid;
pub mod semigroup;

pub mod prelude {
    pub use super::monoid::*;
    pub use super::semigroup::*;
}
