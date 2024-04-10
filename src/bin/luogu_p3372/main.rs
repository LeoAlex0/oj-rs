extern crate solution;

use std::io::stdin;

use solution::seg_tree::*;
use solution::traits::monoid::*;
use solution::traits::semigroup::*;

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
    let mut buf = String::new();
    stdin().read_line(&mut buf).unwrap();

    let [len, num_commands] = match buf
        .split_whitespace()
        .map(|x| x.parse::<u64>().unwrap())
        .take(2)
        .collect::<Vec<_>>()[..]
    {
        [n, m] => [n, m],
        _ => unreachable!(),
    };
    buf.clear();

    stdin().read_line(&mut buf).unwrap();
    let init: Vec<_> = buf
        .split_whitespace()
        .map(|x| x.parse::<i64>().unwrap())
        .collect();
    buf.clear();

    let mut tree: SegTree<_, Plus> =
        SegTree::build(len as usize, |i| (Sum(init[i]), Size::default()));

    for _i in 0..num_commands {
        stdin().read_line(&mut buf).unwrap();
        match buf
            .split_whitespace()
            .map(|x| x.parse::<i64>().unwrap())
            .collect::<Vec<_>>()[..]
        {
            [1, x, y, k] => tree = tree.apply((x - 1) as usize..y as usize, Plus(k)),
            [2, x, y] => println!("{}", tree.query((x - 1) as usize..y as usize).0 .0),
            _ => unreachable!(),
        }
        buf.clear();
    }
}
