//! Classic DPLL: unit propagation + chronological backtracking.
//! Simple and slow; kept as a baseline and a cross-check for CDCL.

use crate::cnf::{Cnf, Lit};
use crate::SolveResult;

pub fn solve(cnf: &Cnf) -> SolveResult {
    let mut assign: Vec<Option<bool>> = vec![None; cnf.num_vars];
    let mut stats = 0u64;
    if rec(cnf, &mut assign, &mut stats) {
        SolveResult::Sat(assign.into_iter().map(|a| a.unwrap_or(false)).collect())
    } else {
        SolveResult::Unsat
    }
}

fn value(assign: &[Option<bool>], l: Lit) -> Option<bool> {
    assign[l.var()].map(|v| v == l.is_positive())
}

/// Assign forced literals until fixpoint. Returns false on conflict.
fn unit_propagate(cnf: &Cnf, assign: &mut [Option<bool>], trail: &mut Vec<usize>) -> bool {
    loop {
        let mut changed = false;
        for c in &cnf.clauses {
            let mut unassigned = None;
            let mut n_unassigned = 0;
            let mut sat = false;
            for &l in c {
                match value(assign, l) {
                    Some(true) => {
                        sat = true;
                        break;
                    }
                    Some(false) => {}
                    None => {
                        n_unassigned += 1;
                        unassigned = Some(l);
                    }
                }
            }
            if sat {
                continue;
            }
            match (n_unassigned, unassigned) {
                (0, _) => return false,
                (1, Some(l)) => {
                    assign[l.var()] = Some(l.is_positive());
                    trail.push(l.var());
                    changed = true;
                }
                _ => {}
            }
        }
        if !changed {
            return true;
        }
    }
}

/// Branch on a variable from the shortest unsatisfied clause.
fn pick(cnf: &Cnf, assign: &[Option<bool>]) -> Option<Lit> {
    let mut best: Option<(usize, Lit)> = None;
    for c in &cnf.clauses {
        if c.iter().any(|&l| value(assign, l) == Some(true)) {
            continue;
        }
        let free: Vec<Lit> = c.iter().copied().filter(|&l| value(assign, l).is_none()).collect();
        if let Some(&l) = free.first() {
            if best.is_none_or(|(n, _)| free.len() < n) {
                best = Some((free.len(), l));
            }
        }
    }
    best.map(|(_, l)| l)
}

fn rec(cnf: &Cnf, assign: &mut [Option<bool>], decisions: &mut u64) -> bool {
    let mut trail = Vec::new();
    if unit_propagate(cnf, assign, &mut trail) {
        let Some(lit) = pick(cnf, assign) else {
            return true; // every clause satisfied
        };
        *decisions += 1;
        for phase in [lit.is_positive(), !lit.is_positive()] {
            assign[lit.var()] = Some(phase);
            if rec(cnf, assign, decisions) {
                return true;
            }
            assign[lit.var()] = None;
        }
    }
    for v in trail {
        assign[v] = None;
    }
    false
}
