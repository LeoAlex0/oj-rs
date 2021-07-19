use luogu::traits::*;
use luogu::seg_tree::*;

use std::io::{stdin, BufRead};

static mut P: u32 = 0;

#[derive(Clone, Debug)]
struct Linear {
    k: u32,
    b: u32,
}

impl Semigroup for Linear {
    /// k2 * (k1 * x + b1) + b2 = (k2 * k1) * x + (k2 * b1 + b2)
    fn merge(self, other: Self) -> Self {
        let p = unsafe { P } as u64;
        Self {
            k: ((self.k as u64 * other.k as u64) % p) as u32,
            b: (((self.k as u64 * other.b as u64) % p + self.b as u64) % p) as u32,
        }
    }
}

impl Monoid for Linear {
    /// x = 1 * x + 0
    fn empty() -> Self {
        Self { k: 1, b: 0 }
    }
}

#[derive(Clone, Debug)]
struct Sum(u32);

#[derive(Clone, Debug)]
struct Size(u32);

impl Semigroup for Sum {
    fn merge(self, other: Self) -> Self {
        Self((self.0 + other.0) % unsafe { P })
    }
}

impl Semigroup for Size {
    fn merge(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Monoid for Sum {
    fn empty() -> Self {
        Self(0)
    }
}

impl Monoid for Size {
    fn empty() -> Self {
        Self(1)
    }
}

impl Applier<(Sum, Size)> for Linear {
    /// x' = k*x + b*n
    fn apply(&self, (Sum(x), Size(n)): (Sum, Size)) -> (Sum, Size) {
        let p = unsafe { P } as u64;
        (
            Sum(((((self.k as u64 * x as u64) % p + (self.b as u64 * n as u64)) % p) % p) as u32),
            Size(n),
        )
    }
}

fn main() {
    let mut buf = String::new();
    let stdin = stdin();
    let mut stdin = stdin.lock();
    stdin.read_line(&mut buf).unwrap();

    let (n, m, p) = match buf
        .split_whitespace()
        .map(|s| s.parse::<u32>().unwrap())
        .collect::<Vec<_>>()[..]
    {
        [n, m, p, ..] => (n, m, p),
        _ => unreachable!(),
    };
    buf.clear();

    // Safe because the tree haven't create yet.
    unsafe {
        P = p;
    }

    stdin.read_line(&mut buf).unwrap();
    let init_value = buf
        .split_whitespace()
        .map(|s| s.parse::<u32>().unwrap())
        .collect::<Vec<_>>();
    buf.clear();

    let mut tree: SegTree<_, Linear> =
        SegTree::build(n as usize, |i| (Sum(init_value[i]), Size(1)));

    for _ in 0..m {
        stdin.read_line(&mut buf).unwrap();
        let op = buf
            .split_whitespace()
            .map(|s| s.parse::<u32>().unwrap())
            .collect::<Vec<_>>();
        buf.clear();

        match op[..] {
            [1, x, y, k] => {
                // a[x..y] = k * a[x..y] + 0
                tree = tree.apply(x as usize - 1..y as usize, Linear { k, b: 0 });
            }
            [2, x, y, b] => {
                // a[x..y] = 1 * a[x..y] + b
                tree = tree.apply(x as usize - 1..y as usize, Linear { k: 1, b });
            }
            [3, x, y] => {
                println!("{}", tree.query(x as usize - 1..y as usize).0 .0);
            }
            _ => unreachable!(),
        }
    }
}
