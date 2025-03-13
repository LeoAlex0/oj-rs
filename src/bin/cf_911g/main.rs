extern crate solution;

use std::io::BufRead;

#[cfg(all(any(target_arch = "x86", target_arch = "x86_64"),))]
#[target_feature(enable = "avx2")]
unsafe fn update(arr: &mut Vec<u8>, l: usize, r: usize, x: u8, y: u8) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;
    let mut p = arr[l - 1..r].as_mut_ptr();
    let end = arr[r..].as_mut_ptr();
    let mx = _mm256_set1_epi8(x as i8);
    let my = _mm256_set1_epi8(y as i8);

    while p.align_offset(32) != 0 && p < end {
        if *p == x {
            *p = y
        }
        p = p.offset(1);
    }
    while p.offset(32) < end {
        let v = _mm256_load_si256(p as *const _);
        let v = _mm256_blendv_epi8(v, my, _mm256_cmpeq_epi8(v, mx));
        _mm256_store_si256(p as *mut _, v);
        p = p.offset(32);
    }
    while p < end {
        if *p == x {
            *p = y
        }
        p = p.offset(1);
    }
}

fn main() {
    let mut lines = std::io::stdin().lock().lines();

    let n: usize = lines.next().unwrap().unwrap().trim().parse().unwrap();

    let mut array: Vec<u8> = lines
        .next()
        .unwrap()
        .unwrap()
        .split_whitespace()
        .map(|word| word.parse().unwrap())
        .collect();

    let _q: usize = lines.next().unwrap().unwrap().trim().parse().unwrap();
    // let mut i: usize = 0;
    while let Some(Ok(line)) = lines.next() {
        if let [l, r, x, y] = line
            .split_whitespace()
            .take(4)
            .map(|word| word.parse::<usize>().unwrap())
            .collect::<Vec<_>>()[..]
        {
            unsafe { update(&mut array, l, r, x as u8, y as u8) }
        }

        // i += 1;
        // if i % 1000 == 0 {
        //     println!("command {i} / {q} done");
        // }
    }

    let ans = array
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(" ");

    println!("{ans}")
}
