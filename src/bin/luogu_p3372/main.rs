use solution::data_structure::seg_tree::{Applier, SegTree, SegTreeStore};
use solution::io::Scanner;
use solution::traits::prelude::*;

#[derive(Clone)]
struct Plus(i64);
impl Semigroup for Plus {
    #[inline]
    fn merge(self, other: Self) -> Self {
        Plus(self.0 + other.0)
    }
}
impl Monoid for Plus {
    #[inline]
    fn empty() -> Self {
        Plus(0)
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.0 == 0
    }
}
impl Applier<(Sum<i64>, Size)> for Plus {
    fn apply(&self, (sum, size): &mut (Sum<i64>, Size)) {
        sum.0 += self.0 * size.0 as i64;
    }
}

fn main() {
    let mut input = Scanner::stdin();
    let len: usize = input.read();
    let num_commands: usize = input.read();
    let init: Vec<i64> = (0..len).map(|_| input.read()).collect();

    let mut store = SegTreeStore::default();
    let mut tree: SegTree<_, Plus> =
        SegTree::build_in(&mut store, len, |i| (Sum(init[i]), Size::default()));

    for _ in 0..num_commands {
        let op: u8 = input.read();
        let x: usize = input.read();
        let y: usize = input.read();
        match op {
            1 => {
                let k: i64 = input.read();
                tree = tree.apply(&mut store, x - 1..y, &Plus(k));
            }
            2 => println!("{}", tree.query(&store, x - 1..y).0 .0),
            _ => unreachable!(),
        }
    }
}
