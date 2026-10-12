// 简易 benchmark，不依赖 criterion，避免增加磁盘和编译负担。
// 运行：cargo bench

use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

use regex::Regex;

fn make_fixture(lines: usize) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("fsearch_bench_{}.txt", std::process::id()));
    let mut content = String::with_capacity(lines * 40);
    for i in 0..lines {
        if i % 100 == 0 {
            content.push_str("fn needle_function() {}\n");
        } else {
            content.push_str("let x = some_value + other_value;\n");
        }
    }
    fs::write(&p, content).unwrap();
    p
}

fn bench_scan(path: &PathBuf, pattern: &str, iterations: usize) -> u128 {
    let re = Regex::new(pattern).unwrap();
    let content = fs::read_to_string(path).unwrap();
    let start = Instant::now();
    for _ in 0..iterations {
        let mut count = 0usize;
        for line in content.lines() {
            if re.is_match(black_box(line)) {
                count += 1;
            }
        }
        black_box(count);
    }
    start.elapsed().as_nanos() / iterations as u128
}

fn main() {
    let path = make_fixture(100_000);
    let iters = 50;

    let literal = bench_scan(&path, "needle_function", iters);
    let regex = bench_scan(&path, r"fn \w+\(\)", iters);
    let nomatch = bench_scan(&path, "very_rare_string_xyz", iters);

    println!("lines: 100_000, iterations: {iters}");
    println!("  literal pattern:  {literal:>12} ns/iter");
    println!("  regex pattern:    {regex:>12} ns/iter");
    println!("  no-match pattern: {nomatch:>12} ns/iter");

    let _ = fs::remove_file(&path);
}