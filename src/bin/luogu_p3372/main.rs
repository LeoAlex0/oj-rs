use solution::data_structure::seg_tree::prelude::*;
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
}
impl Applier<(Sum<i64>, Size)> for Plus {
    fn apply(&self, (Sum(s), n): (Sum<i64>, Size)) -> (Sum<i64>, Size) {
        (Sum(s + self.0 * n.0 as i64), n)
    }
}

fn main() {
    let mut input = Scanner::stdin();
    let len: usize = input.next();
    let num_commands: usize = input.next();
    let init: Vec<i64> = (0..len).map(|_| input.next()).collect();

    let mut tree: SegTree<_, Plus> = SegTree::build(len, |i| (Sum(init[i]), Size::default()));

    for _ in 0..num_commands {
        let op: u8 = input.next();
        let x: usize = input.next();
        let y: usize = input.next();
        match op {
            1 => {
                let k: i64 = input.next();
                tree = tree.apply(x - 1..y, Plus(k));
            }
            2 => println!("{}", tree.query(x - 1..y).0 .0),
            _ => unreachable!(),
        }
    }
}
