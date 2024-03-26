//! Instance generators used by tests, benchmarks and the CLI.

use crate::cnf::{Cnf, Lit};

/// Pigeonhole principle: `pigeons` pigeons into `holes` holes, each hole
/// holding at most one. Unsatisfiable iff pigeons > holes, and exponentially
/// hard for resolution-based solvers.
pub fn pigeonhole(pigeons: usize, holes: usize) -> Cnf {
    let var = |p: usize, h: usize| p * holes + h;
    let mut cnf = Cnf::new(pigeons * holes);
    for p in 0..pigeons {
        cnf.add((0..holes).map(|h| Lit::new(var(p, h), true)));
    }
    for h in 0..holes {
        for p1 in 0..pigeons {
            for p2 in p1 + 1..pigeons {
                cnf.add([Lit::new(var(p1, h), false), Lit::new(var(p2, h), false)]);
            }
        }
    }
    cnf
}

/// Uniform random k-SAT with a deterministic xorshift PRNG.
pub fn random_ksat(vars: usize, clauses: usize, k: usize, seed: u64) -> Cnf {
    let mut x = seed.max(1);
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let mut cnf = Cnf::new(vars);
    for _ in 0..clauses {
        let mut c: Vec<Lit> = Vec::with_capacity(k);
        while c.len() < k {
            let v = (next() % vars as u64) as usize;
            if c.iter().any(|l| l.var() == v) {
                continue;
            }
            c.push(Lit::new(v, next() & 1 == 0));
        }
        cnf.add(c);
    }
    cnf
}

/// N-queens: one queen per row, no two attacking. SAT for n = 1 and n >= 4.
pub fn queens(n: usize) -> Cnf {
    let var = |r: usize, c: usize| r * n + c;
    let mut cnf = Cnf::new(n * n);
    for r in 0..n {
        cnf.add((0..n).map(|c| Lit::new(var(r, c), true)));
    }
    let cells: Vec<(usize, usize)> = (0..n).flat_map(|r| (0..n).map(move |c| (r, c))).collect();
    for (i, &(r1, c1)) in cells.iter().enumerate() {
        for &(r2, c2) in &cells[i + 1..] {
            if r1 == r2 || c1 == c2 || r1.abs_diff(r2) == c1.abs_diff(c2) {
                cnf.add([Lit::new(var(r1, c1), false), Lit::new(var(r2, c2), false)]);
            }
        }
    }
    cnf
}
