use criterion::{black_box, Criterion};
use solution::{
    data_structure::{
        ref_store::{AlignedArenaStoreFactory, ArenaRef, ArenaStoreFactory},
        seg_tree::{
            seg_block_capacity_for_bytes, Applier, SegTree, SegTreeStore, SEG_TREE_CACHE_LINE_BYTES,
        },
    },
    traits::prelude::{Monoid, Semigroup},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mask(u128);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Color(u8);

#[derive(Clone)]
struct Replace {
    map: [u8; 101],
    changed: u128,
    identity: bool,
}

const N: usize = 200_000;
const OPS: usize = 1 << 15;
const MASK_LEAF_BLOCK: usize =
    seg_block_capacity_for_bytes::<Mask, Replace, ArenaRef<'static>>(SEG_TREE_CACHE_LINE_BYTES);
const COLOR_LEAF_BLOCK: usize =
    seg_block_capacity_for_bytes::<Color, Replace, ArenaRef<'static>>(SEG_TREE_CACHE_LINE_BYTES);

impl Replace {
    fn new(from: u8, to: u8) -> Self {
        let mut map = [0; 101];
        for (i, value) in map.iter_mut().enumerate() {
            *value = i as u8;
        }
        map[from as usize] = to;
        Self {
            map,
            changed: u128::from(from != to) << from,
            identity: from == to,
        }
    }

    fn apply_to_mask(&self, mask: Mask) -> Mask {
        if self.identity {
            return mask;
        }

        let mut next = 0;
        let mut rest = mask.0;
        while rest != 0 {
            let color = rest.trailing_zeros() as usize;
            next |= 1u128 << self.map[color];
            rest &= rest - 1;
        }
        Mask(next)
    }

    fn apply_to_color(&self, color: Color) -> Color {
        if self.identity || color.0 == 0 {
            color
        } else {
            Color(self.map[color.0 as usize])
        }
    }
}

impl Semigroup for Mask {
    fn merge(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    fn merge_assign(&mut self, other: &Self) {
        self.0 |= other.0;
    }
}

impl Monoid for Mask {
    fn empty() -> Self {
        Self(0)
    }

    fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

impl Semigroup for Color {
    fn merge(self, other: Self) -> Self {
        if other.0 == 0 {
            self
        } else {
            other
        }
    }

    fn merge_assign(&mut self, other: &Self) {
        if other.0 != 0 {
            *self = *other;
        }
    }
}

impl Monoid for Color {
    fn empty() -> Self {
        Self(0)
    }

    fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

impl Semigroup for Replace {
    fn merge(self, other: Self) -> Self {
        if self.identity {
            return other;
        }
        if other.identity {
            return self;
        }

        let mut map = [0; 101];
        let mut changed = 0;
        let mut identity = true;
        for (i, value) in map.iter_mut().enumerate() {
            *value = self.map[other.map[i] as usize];
            if *value == i as u8 {
                continue;
            }
            changed |= 1u128 << i;
            identity = false;
        }
        Self {
            map,
            changed,
            identity,
        }
    }

    fn prepend_assign(&mut self, other: &Self)
    where
        Self: Clone,
    {
        if other.identity {
            return;
        }
        if self.identity {
            *self = other.clone();
            return;
        }

        let mut changed = 0;
        let mut identity = true;
        for (i, value) in self.map.iter_mut().enumerate() {
            *value = other.map[*value as usize];
            if *value == i as u8 {
                continue;
            }
            changed |= 1u128 << i;
            identity = false;
        }
        self.changed = changed;
        self.identity = identity;
    }
}

impl Monoid for Replace {
    fn empty() -> Self {
        let mut map = [0; 101];
        for (i, value) in map.iter_mut().enumerate() {
            *value = i as u8;
        }
        Self {
            map,
            changed: 0,
            identity: true,
        }
    }

    fn is_empty(&self) -> bool {
        self.identity
    }
}

impl Applier<Mask> for Replace {
    fn apply(&self, mask: &mut Mask) {
        *mask = self.apply_to_mask(*mask);
    }

    fn affects(&self, value: &Mask) -> bool {
        if self.identity {
            return false;
        }

        value.0 & self.changed != 0
    }
}

impl Applier<Color> for Replace {
    fn apply(&self, color: &mut Color) {
        *color = self.apply_to_color(*color);
    }
}

fn mask(value: u8) -> Mask {
    Mask(1u128 << value)
}

fn initial_color(index: usize) -> u8 {
    (index % 100 + 1) as u8
}

fn initial_array() -> Vec<u8> {
    (0..N).map(initial_color).collect()
}

fn random_ranges() -> Vec<(usize, usize)> {
    let mut seed = 0x9119_1919_5eed_u64;
    let mut next = || {
        seed ^= seed << 7;
        seed ^= seed >> 9;
        seed ^= seed << 8;
        seed as usize
    };

    (0..OPS)
        .map(|_| {
            let a = next() % N + 1;
            let b = next() % N + 1;
            if a <= b {
                (a, b)
            } else {
                (b, a)
            }
        })
        .collect()
}

fn random_ops() -> Vec<(usize, usize, u8, u8)> {
    let mut seed = 0x0091_1919_19c0_11e7_u64;
    let mut next = || {
        seed ^= seed << 7;
        seed ^= seed >> 9;
        seed ^= seed << 8;
        seed as usize
    };

    (0..OPS)
        .map(|_| {
            let a = next() % N + 1;
            let b = next() % N + 1;
            let x = (next() % 100 + 1) as u8;
            let mut y = (next() % 100 + 1) as u8;
            if x == y {
                y = y % 100 + 1;
            }
            let (l, r) = if a <= b { (a, b) } else { (b, a) };
            (l, r, x, y)
        })
        .collect()
}

fn scalar_update(arr: &mut [u8], l: usize, r: usize, x: u8, y: u8) {
    for value in &mut arr[l - 1..r] {
        if *value == x {
            *value = y;
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn simd_update_avx2(arr: &mut [u8], l: usize, r: usize, x: u8, y: u8) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let mut p = arr[l - 1..r].as_mut_ptr();
    let end = arr[r..].as_mut_ptr();
    let mx = _mm256_set1_epi8(x as i8);
    let my = _mm256_set1_epi8(y as i8);

    while p < end && p.align_offset(32) != 0 {
        if *p == x {
            *p = y;
        }
        p = p.add(1);
    }
    while (end as usize).wrapping_sub(p as usize) >= 32 {
        let v = _mm256_load_si256(p as *const _);
        let v = _mm256_blendv_epi8(v, my, _mm256_cmpeq_epi8(v, mx));
        _mm256_store_si256(p as *mut _, v);
        p = p.add(32);
    }
    while p < end {
        if *p == x {
            *p = y;
        }
        p = p.add(1);
    }
}

fn simd_update(arr: &mut [u8], l: usize, r: usize, x: u8, y: u8) {
    if x == y {
        return;
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::is_x86_feature_detected!("avx2") {
            unsafe {
                simd_update_avx2(arr, l, r, x, y);
            }
            return;
        }
    }
    scalar_update(arr, l, r, x, y);
}

fn assert_seg_tree_matches_bruteforce() {
    const SMALL_N: usize = 257;
    const SMALL_OPS: usize = 1024;

    ArenaStoreFactory::scoped(SMALL_N * 2, |factory| {
        let mut store: SegTreeStore<Mask, Replace, MASK_LEAF_BLOCK, _> = SegTreeStore::new(factory);
        let mut tree = SegTree::build_in(&mut store, SMALL_N, |i| mask(initial_color(i)));
        let mut values = (0..SMALL_N).map(initial_color).collect::<Vec<_>>();
        let ops = random_ops();

        for &(l, r, x, y) in ops.iter().take(SMALL_OPS) {
            let l = l % SMALL_N;
            let r = r % SMALL_N;
            let (l, r) = if l <= r { (l, r + 1) } else { (r, l + 1) };
            for value in &mut values[l..r] {
                if *value == x {
                    *value = y;
                }
            }
            tree.apply_mut(&mut store, l..r, &Replace::new(x, y));
        }

        let actual = tree
            .iter(&store)
            .map(|value| value.0.trailing_zeros() as u8)
            .collect::<Vec<_>>();
        assert_eq!(actual, values);
    });

    ArenaStoreFactory::scoped(SMALL_N * 2, |factory| {
        let mut store: SegTreeStore<Color, Replace, COLOR_LEAF_BLOCK, _> =
            SegTreeStore::new(factory);
        let mut tree = SegTree::build_in(&mut store, SMALL_N, |i| Color(initial_color(i)));
        let mut values = (0..SMALL_N).map(initial_color).collect::<Vec<_>>();
        let ops = random_ops();

        for &(l, r, x, y) in ops.iter().take(SMALL_OPS) {
            let l = l % SMALL_N;
            let r = r % SMALL_N;
            let (l, r) = if l <= r { (l, r + 1) } else { (r, l + 1) };
            for value in &mut values[l..r] {
                if *value == x {
                    *value = y;
                }
            }
            tree.apply_mut(&mut store, l..r, &Replace::new(x, y));
        }

        let actual = tree.iter(&store).map(|value| value.0).collect::<Vec<_>>();
        assert_eq!(actual, values);
    });
}

pub fn bench(c: &mut Criterion) {
    assert_seg_tree_matches_bruteforce();

    let mut group = c.benchmark_group("cf_911g");

    group.bench_function("simd update large", |b| {
        let mut array = vec![1u8; N];
        b.iter(|| {
            simd_update(
                black_box(&mut array),
                black_box(1),
                black_box(N - 1),
                black_box(1),
                black_box(2),
            )
        })
    });

    group.bench_function("segtree update large", |b| {
        let mut store = SegTreeStore::default();
        let tree: SegTree<_, Replace, MASK_LEAF_BLOCK> =
            SegTree::build_in(&mut store, N, |_| mask(1));
        b.iter(|| {
            let modifier = black_box(Replace::new(1, 2));
            black_box(black_box(tree.clone()).apply(&mut store, black_box(1..N - 1), &modifier))
        })
    });

    group.bench_function("segtree update large mut arena", |b| {
        ArenaStoreFactory::scoped(N * 2, |factory| {
            let mut store: SegTreeStore<Mask, Replace, MASK_LEAF_BLOCK, _> =
                SegTreeStore::new(factory);
            let mut tree = SegTree::build_in(&mut store, N, |_| mask(1));
            b.iter(|| {
                let modifier = black_box(Replace::new(1, 2));
                tree.apply_mut(&mut store, black_box(1..N - 1), &modifier);
                black_box(&tree);
            })
        })
    });

    group.bench_function("segtree update large mut aligned arena", |b| {
        AlignedArenaStoreFactory::scoped(N * 2, |factory| {
            let mut store: SegTreeStore<Mask, Replace, MASK_LEAF_BLOCK, _> =
                SegTreeStore::new(factory);
            let mut tree = SegTree::build_in(&mut store, N, |_| mask(1));
            b.iter(|| {
                let modifier = black_box(Replace::new(1, 2));
                tree.apply_mut(&mut store, black_box(1..N - 1), &modifier);
                black_box(&tree);
            })
        })
    });

    group.bench_function("segtree color update large mut aligned arena", |b| {
        AlignedArenaStoreFactory::scoped(N * 2, |factory| {
            let mut store: SegTreeStore<Color, Replace, COLOR_LEAF_BLOCK, _> =
                SegTreeStore::new(factory);
            let mut tree = SegTree::build_in(&mut store, N, |_| Color(1));
            b.iter(|| {
                let modifier = black_box(Replace::new(1, 2));
                tree.apply_mut(&mut store, black_box(1..N - 1), &modifier);
                black_box(&tree);
            })
        })
    });

    group.bench_function("simd update random", |b| {
        let mut array = vec![1u8; N];
        let ranges = random_ranges();
        let mut index = 0usize;
        b.iter(|| {
            let (l, r) = ranges[index & (OPS - 1)];
            index += 1;
            simd_update(
                black_box(&mut array),
                black_box(l),
                black_box(r),
                black_box(1),
                black_box(2),
            )
        })
    });

    group.bench_function("segtree update random", |b| {
        let mut store = SegTreeStore::default();
        let tree: SegTree<_, Replace, MASK_LEAF_BLOCK> =
            SegTree::build_in(&mut store, N, |_| mask(1));
        let ranges = random_ranges();
        let mut index = 0usize;
        b.iter(|| {
            let (l, r) = ranges[index & (OPS - 1)];
            index += 1;
            let modifier = black_box(Replace::new(1, 2));
            black_box(black_box(tree.clone()).apply(&mut store, black_box(l - 1..r), &modifier))
        })
    });

    group.bench_function("segtree update random mut aligned arena", |b| {
        AlignedArenaStoreFactory::scoped(N * 2, |factory| {
            let mut store: SegTreeStore<Mask, Replace, MASK_LEAF_BLOCK, _> =
                SegTreeStore::new(factory);
            let mut tree = SegTree::build_in(&mut store, N, |_| mask(1));
            let ranges = random_ranges();
            let mut index = 0usize;
            b.iter(|| {
                let (l, r) = ranges[index & (OPS - 1)];
                index += 1;
                let modifier = black_box(Replace::new(1, 2));
                tree.apply_mut(&mut store, black_box(l - 1..r), &modifier);
                black_box(&tree);
            })
        })
    });

    group.bench_function("segtree color update random mut aligned arena", |b| {
        AlignedArenaStoreFactory::scoped(N * 2, |factory| {
            let mut store: SegTreeStore<Color, Replace, COLOR_LEAF_BLOCK, _> =
                SegTreeStore::new(factory);
            let mut tree = SegTree::build_in(&mut store, N, |_| Color(1));
            let ranges = random_ranges();
            let mut index = 0usize;
            b.iter(|| {
                let (l, r) = ranges[index & (OPS - 1)];
                index += 1;
                let modifier = black_box(Replace::new(1, 2));
                tree.apply_mut(&mut store, black_box(l - 1..r), &modifier);
                black_box(&tree);
            })
        })
    });

    group.bench_function("simd update alternating full", |b| {
        let mut array = vec![1u8; N];
        let mut flip = false;
        b.iter(|| {
            let (x, y) = if flip { (2, 1) } else { (1, 2) };
            flip = !flip;
            simd_update(
                black_box(&mut array),
                black_box(1),
                black_box(N),
                black_box(x),
                black_box(y),
            )
        })
    });

    group.bench_function("segtree update alternating full mut aligned arena", |b| {
        AlignedArenaStoreFactory::scoped(N * 2, |factory| {
            let mut store: SegTreeStore<Mask, Replace, MASK_LEAF_BLOCK, _> =
                SegTreeStore::new(factory);
            let mut tree = SegTree::build_in(&mut store, N, |_| mask(1));
            let mut flip = false;
            b.iter(|| {
                let (from, to) = if flip { (2, 1) } else { (1, 2) };
                flip = !flip;
                tree.apply_mut(&mut store, black_box(0..N), &Replace::new(from, to));
                black_box(&tree);
            })
        })
    });

    group.bench_function(
        "segtree color update alternating full mut aligned arena",
        |b| {
            AlignedArenaStoreFactory::scoped(N * 2, |factory| {
                let mut store: SegTreeStore<Color, Replace, COLOR_LEAF_BLOCK, _> =
                    SegTreeStore::new(factory);
                let mut tree = SegTree::build_in(&mut store, N, |_| Color(1));
                let mut flip = false;
                b.iter(|| {
                    let (from, to) = if flip { (2, 1) } else { (1, 2) };
                    flip = !flip;
                    tree.apply_mut(&mut store, black_box(0..N), &Replace::new(from, to));
                    black_box(&tree);
                })
            })
        },
    );

    group.bench_function("simd update mixed", |b| {
        let mut array = initial_array();
        let ops = random_ops();
        let mut index = 0usize;
        b.iter(|| {
            let (l, r, x, y) = ops[index & (OPS - 1)];
            index += 1;
            simd_update(
                black_box(&mut array),
                black_box(l),
                black_box(r),
                black_box(x),
                black_box(y),
            )
        })
    });

    group.bench_function("segtree update mixed mut aligned arena", |b| {
        AlignedArenaStoreFactory::scoped(N * 2, |factory| {
            let mut store: SegTreeStore<Mask, Replace, MASK_LEAF_BLOCK, _> =
                SegTreeStore::new(factory);
            let mut tree = SegTree::build_in(&mut store, N, |i| mask(initial_color(i)));
            let ops = random_ops();
            let mut index = 0usize;
            b.iter(|| {
                let (l, r, x, y) = ops[index & (OPS - 1)];
                index += 1;
                let modifier = black_box(Replace::new(x, y));
                tree.apply_mut(&mut store, black_box(l - 1..r), &modifier);
                black_box(&tree);
            })
        })
    });

    group.bench_function("segtree color update mixed mut aligned arena", |b| {
        AlignedArenaStoreFactory::scoped(N * 2, |factory| {
            let mut store: SegTreeStore<Color, Replace, COLOR_LEAF_BLOCK, _> =
                SegTreeStore::new(factory);
            let mut tree = SegTree::build_in(&mut store, N, |i| Color(initial_color(i)));
            let ops = random_ops();
            let mut index = 0usize;
            b.iter(|| {
                let (l, r, x, y) = ops[index & (OPS - 1)];
                index += 1;
                let modifier = black_box(Replace::new(x, y));
                tree.apply_mut(&mut store, black_box(l - 1..r), &modifier);
                black_box(&tree);
            })
        })
    });

    group.finish();
}
