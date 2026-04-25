use captcha_core::{challenge::Challenge, pow};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn make_challenge(diff: u8) -> Challenge {
    Challenge {
        id: "bench-challenge-id-001".to_string(),
        salt: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        diff,
        exp: u64::MAX,
        site_key: "pk_bench".to_string(),
        m_cost: 4096,
        t_cost: 1,
        p_cost: 1,
    }
}

fn bench_compute_base_hash(c: &mut Criterion) {
    let ch = make_challenge(18);
    c.bench_function("compute_base_hash (Argon2id 4MiB)", |b| {
        b.iter(|| pow::compute_base_hash(black_box(&ch)))
    });
}

fn bench_compute_pow_hash(c: &mut Criterion) {
    let ch = make_challenge(18);
    let base = pow::compute_base_hash(&ch);
    c.bench_function("compute_pow_hash (SHA-256)", |b| {
        b.iter(|| pow::compute_pow_hash(black_box(&base), black_box(42)))
    });
}

fn bench_verify_solution(c: &mut Criterion) {
    let ch = make_challenge(8);
    let (nonce, _) = pow::solve(&ch, 1_000_000, 0, |_| {}).unwrap();
    c.bench_function("verify_solution (Argon2 + SHA-256)", |b| {
        b.iter(|| pow::verify_solution(black_box(&ch), black_box(nonce)))
    });
}

fn bench_solve_diff_8(c: &mut Criterion) {
    let ch = make_challenge(8);
    c.bench_function("solve diff=8", |b| {
        b.iter(|| pow::solve(black_box(&ch), 1_000_000, 0, |_| {}))
    });
}

criterion_group!(
    benches,
    bench_compute_base_hash,
    bench_compute_pow_hash,
    bench_verify_solution,
    bench_solve_diff_8,
);
criterion_main!(benches);
