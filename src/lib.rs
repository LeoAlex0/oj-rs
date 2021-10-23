#![feature(test)]

pub mod traits;

pub mod data_structure;
pub use data_structure::seg_tree;

#[cfg(test)]
pub mod test;
