extern crate solution;

use std::{array, io::BufRead};

#[derive(Clone)]
struct TransposeU8([u8; u8::MAX as usize + 1]);
impl solution::traits::semigroup::Semigroup for TransposeU8 {
    fn merge(self, other: Self) -> Self {
        Self(other.0.map(|it| self.0[it as usize]))
    }
}

impl TransposeU8 {
    fn assign(mut self, x: u8, y: u8) -> Self {
        self.0[x as usize] = y;
        self
    }

    fn identity() -> Self {
        Self(array::from_fn(|i| i as u8)) // identity transpose
    }
}

impl solution::traits::monoid::Monoid for TransposeU8 {
    fn empty() -> Self {
        Self::identity()
    }
}

impl solution::data_structure::seg_tree::Applier<Option<solution::traits::semigroup::Identity<u8>>>
    for TransposeU8
{
    fn apply(
        &self,
        to: Option<solution::traits::semigroup::Identity<u8>>,
    ) -> Option<solution::traits::semigroup::Identity<u8>> {
        to.map(|to| solution::traits::semigroup::Identity(self.0[to.0 as usize]))
    }
}

fn main() {
    let mut lines = std::io::stdin().lock().lines();

    let n: usize = lines.next().unwrap().unwrap().trim().parse().unwrap();

    let array: Vec<u8> = lines
        .next()
        .unwrap()
        .unwrap()
        .split_whitespace()
        .map(|word| word.parse().unwrap())
        .collect();
    let mut tree: solution::data_structure::seg_tree::SegTree<_, TransposeU8> =
        solution::data_structure::seg_tree::SegTree::build(n, |i| {
            Some(solution::traits::semigroup::Identity(array[i]))
        });
    drop(array);

    let q: u32 = lines.next().unwrap().unwrap().trim().parse().unwrap();
    for _ in 0..q {
        if let [l, r, x, y] = lines
            .next()
            .unwrap()
            .unwrap()
            .split_whitespace()
            .take(4)
            .map(|word| word.parse::<usize>().unwrap())
            .collect::<Vec<_>>()[..]
        {
            tree = tree.apply(l - 1..r, TransposeU8::identity().assign(x as u8, y as u8));
        }
    }

    let ans = (0..n)
        .map(|i| tree.query(i..i + 1).unwrap().0.to_string())
        .collect::<Vec<_>>()
        .join(" ");

    println!("{ans}")
}
