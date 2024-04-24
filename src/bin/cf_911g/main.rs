extern crate solution;

use std::{
    collections::{BTreeMap, BTreeSet},
    io::BufRead,
};

#[derive(Clone)]
struct TransposeU8(BTreeMap<u8, u8>);
impl solution::traits::semigroup::Semigroup for TransposeU8 {
    fn merge(self, other: Self) -> Self {
        // Self(other.0.map(|it| self.0[it as usize]))
        let intrested_keys = self.0.keys().chain(other.0.keys()).collect::<BTreeSet<_>>();

        Self(
            intrested_keys
                .into_iter()
                .filter_map(|l| {
                    let m = other.0.get(l).unwrap_or(l);
                    let r = self.0.get(m).unwrap_or(m);

                    if *l == *r {
                        None
                    } else {
                        Some((*l, *r))
                    }
                })
                .collect(),
        )
    }
}

impl TransposeU8 {
    fn assign(mut self, x: u8, y: u8) -> Self {
        self.0.insert(x, y);
        self
    }

    fn identity() -> Self {
        Self(BTreeMap::new()) // identity transpose
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
        to.map(|to| solution::traits::semigroup::Identity(*self.0.get(&to.0).unwrap_or(&to.0)))
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

    let q: usize = lines.next().unwrap().unwrap().trim().parse().unwrap();
    let mut i: usize = 0;
    while let Some(Ok(line)) = lines.next() {
        if let [l, r, x, y] = line
            .split_whitespace()
            .take(4)
            .map(|word| word.parse::<usize>().unwrap())
            .collect::<Vec<_>>()[..]
        {
            tree = tree.apply(l - 1..r, TransposeU8::identity().assign(x as u8, y as u8));
        }

        i += 1;
        if i % 10000 == 0 {
            println!("command {i} / {q} done");
        }
    }

    let ans = tree
        .iter()
        .map(|i| i.unwrap().0.to_string())
        .collect::<Vec<_>>()
        .join(" ");

    println!("{ans}")
}
