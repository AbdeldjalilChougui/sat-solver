//! A SAT solver written from scratch: [`dpll`] (baseline) and [`cdcl`]
//! (watched literals, clause learning, VSIDS, restarts), with DIMACS
//! I/O ([`cnf`]) and a Sudoku encoder ([`sudoku`]).

pub mod cdcl;
pub mod cnf;
pub mod dpll;
pub mod gen;
pub mod sudoku;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolveResult {
    /// A satisfying assignment, indexed by variable.
    Sat(Vec<bool>),
    Unsat,
}

impl SolveResult {
    pub fn is_sat(&self) -> bool {
        matches!(self, SolveResult::Sat(_))
    }
}
