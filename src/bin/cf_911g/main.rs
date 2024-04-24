extern crate solution;

use std::{array, borrow::Cow, io::BufRead, sync::OnceLock};

#[derive(Clone, PartialEq, Eq)]
struct TransposeU8<'a>(Cow<'a, [u8; u8::MAX as usize + 1]>);
impl<'a> solution::traits::semigroup::Semigroup for TransposeU8<'a> {
    fn merge(mut self, other: Self) -> Self {
        let identity = IDENTITY_TRANSPOSE.get().unwrap();
        if other == *identity {
            return self;
        }
        if self == *identity {
            return other;
        }

        for l in u8::MIN..=u8::MAX {
            let m = other.0[l as usize];
            if l == m {
                continue;
            }
            if self.0[l as usize] == self.0[m as usize] {
                continue;
            }
            self.0.to_mut()[l as usize] = self.0[m as usize];
        }

        self
        // let intrested_keys = self.0.keys().chain(other.0.keys()).collect::<BTreeSet<_>>();

        // Self(
        //     intrested_keys
        //         .into_iter()
        //         .filter_map(|l| {
        //             let m = other.0.get(l).unwrap_or(l);
        //             let r = self.0.get(m).unwrap_or(m);

        //             if *l == *r {
        //                 None
        //             } else {
        //                 Some((*l, *r))
        //             }
        //         })
        //         .collect(),
        // )
    }
}

static IDENTITY_TRANSPOSE: OnceLock<TransposeU8> = OnceLock::new();

impl<'a> TransposeU8<'a> {
    fn assign(mut self, x: u8, y: u8) -> Self {
        self.0.to_mut()[x as usize] = y;
        self
    }

    fn identity() -> Self {
        IDENTITY_TRANSPOSE
            .get_or_init(|| TransposeU8(Cow::Owned(array::from_fn(|i| i as u8))))
            .clone()
        // Self(array::from_fn(|i| i as u8)) // identity transpose
    }
}

impl<'a> solution::traits::monoid::Monoid for TransposeU8<'a> {
    fn empty() -> Self {
        Self::identity()
    }
}

impl<'a>
    solution::data_structure::seg_tree::Applier<Option<solution::traits::semigroup::Identity<u8>>>
    for TransposeU8<'a>
{
    fn apply(
        &self,
        to: Option<solution::traits::semigroup::Identity<u8>>,
    ) -> Option<solution::traits::semigroup::Identity<u8>> {
        // to.map(|to| solution::traits::semigroup::Identity(*self.0.get(&to.0).unwrap_or(&to.0)))
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

    let _q: usize = lines.next().unwrap().unwrap().trim().parse().unwrap();
    // let mut i: usize = 0;
    while let Some(Ok(line)) = lines.next() {
        if let [l, r, x, y] = line
            .split_whitespace()
            .take(4)
            .map(|word| word.parse::<usize>().unwrap())
            .collect::<Vec<_>>()[..]
        {
            tree = tree.apply(l - 1..r, TransposeU8::identity().assign(x as u8, y as u8));
        }

        // i += 1;
        // if i % 1000 == 0 {
        //     println!("command {i} / {q} done");
        // }
    }

    let ans = tree
        .iter()
        .map(|i| i.unwrap().0.to_string())
        .collect::<Vec<_>>()
        .join(" ");

    println!("{ans}")
}
