use std::cmp::max;
use std::io::{stdin, stdout, Result, Write};
use std::iter::Iterator;

fn main() -> Result<()> {
    let mut buf = String::new();
    stdin().read_line(&mut String::new()).ok();
    stdin().read_line(&mut buf).ok();

    let (ans, _) = buf
        .split_whitespace()
        .map(|word| word.parse().unwrap())
        .zip(0usize..)
        .fold((0u64, Vec::new()), |(ans, mut que), (cur, i)| {
            que.push((i, cur));
            let (new_ans, new_que): (Vec<_>, _) = que
                .iter()
                .scan(0, |last_gcd, q| match gcd(q.1, cur) {
                    cur_gcd if cur_gcd != *last_gcd => {
                        *last_gcd = cur_gcd;
                        let ans = (i + 1 - q.0) as u64 * cur_gcd;
                        Some(Some((ans, (q.0, cur_gcd))))
                    }
                    _ => Some(None),
                })
                .flatten()
                .unzip();
            (max(ans, *new_ans.iter().max().unwrap()), new_que)
        });
    stdout().write_all(ans.to_string().as_bytes())
}

fn gcd(a: u64, b: u64) -> u64 {
    match a {
        0 => b,
        _ => gcd(b % a, a),
    }
}
