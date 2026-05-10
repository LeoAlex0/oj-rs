use solution::data_structure::seg_tree::prelude::*;
use solution::io::Scanner;
use solution::traits::prelude::{Monoid, Semigroup};

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
    let mut input = Scanner::stdin();
    let len: usize = input.next();
    let num_commands: usize = input.next();
    let mod_by: u32 = input.next();

    // Safe because the tree haven't create yet.
    unsafe {
        P = mod_by;
    }

    let init_value: Vec<u32> = (0..len).map(|_| input.next()).collect();

    let mut tree: SegTree<_, Linear> = SegTree::build(len, |i| (Sum(init_value[i]), Size(1)));

    for _ in 0..num_commands {
        let op: u8 = input.next();
        let x: usize = input.next();
        let y: usize = input.next();
        match op {
            1 => {
                let k: u32 = input.next();
                // a[x..y] = k * a[x..y] + 0
                tree = tree.apply(x - 1..y, Linear { k, b: 0 });
            }
            2 => {
                let b: u32 = input.next();
                // a[x..y] = 1 * a[x..y] + b
                tree = tree.apply(x - 1..y, Linear { k: 1, b });
            }
            3 => {
                println!("{}", tree.query(x - 1..y).0 .0);
            }
            _ => unreachable!(),
        }
    }
}
