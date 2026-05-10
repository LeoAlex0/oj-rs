use solution::io::{Output, Scanner};

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn update(arr: &mut [u8], l: usize, r: usize, x: u8, y: u8) {
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
    let mut input = Scanner::stdin();

    let n: usize = input.read();

    let mut array: Vec<u8> = (0..n).map(|_| input.read()).collect();

    let q: usize = input.read();
    for _ in 0..q {
        let l: usize = input.read();
        let r: usize = input.read();
        let x: u8 = input.read();
        let y: u8 = input.read();
        unsafe { update(&mut array, l, r, x, y) }
    }

    let mut output = Output::stdout();
    for (i, value) in array.iter().enumerate() {
        if i > 0 {
            output.print(" ");
        }
        output.print(value);
    }
    output.println("");
}
