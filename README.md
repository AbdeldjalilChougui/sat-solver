# sat-solver

A boolean satisfiability solver written from scratch in Rust, in two steps: classic
**DPLL**, then **CDCL** in the style of MiniSat. It reads and writes DIMACS CNF and
includes a Sudoku solver as a demo.

## Quick start

```sh
cargo build --release
S=target/release/sat

$S solve tests/cnf/unsat_php_7_6.cnf          # exit code 10 = SAT, 20 = UNSAT
$S solve --dpll tests/cnf/sat_queens_10.cnf   # use the DPLL baseline
$S gen random 200 852 42 | $S solve -         # random 3-SAT near the phase transition
$S gen php 9 8 > php.cnf                      # pigeonhole instance (UNSAT)
$S sudoku "8..........36......7..9.2...5...7.......457.....1...3...1....68..85...1..9....4.."
```

Output follows the SAT competition format (`s SATISFIABLE` and `v ... 0` lines).
Every model is checked against the formula before it is printed.

## How it works

- `cnf.rs`: literals are packed as `2*var + sign`. DIMACS parsing handles comments,
  clauses that span lines, and the SATLIB `%` end marker.
- `dpll.rs`: unit propagation to a fixpoint, branching on a literal from the shortest
  unsatisfied clause, and chronological backtracking. It is the baseline the CDCL
  solver is checked against.
- `cdcl.rs`:
  - **Two watched literals** with blocker literals, so most satisfied clauses are
    skipped without touching clause memory.
  - **1-UIP conflict analysis** with local clause minimization, and
    **non-chronological backjumping** to the second-highest level in the learnt clause.
  - **VSIDS** variable activity, kept in an indexed binary heap, plus phase saving.
  - **Luby restarts** (unit of 100 conflicts).
  - **Learnt clause deletion**: the less active half is dropped once the database
    passes a growing limit (binary and locked clauses are kept), and then garbage
    collected.
- `sudoku.rs`: 729 variables ("cell (r,c) holds d"), exactly-one constraints per cell,
  row, column and box, and the givens as unit clauses. `is_unique` blocks the first
  solution and solves again.
- `gen.rs`: generators for pigeonhole (UNSAT when pigeons > holes), n-queens and
  uniform random k-SAT.

## Tests

`cargo test` covers:
- the known instances in `tests/cnf/` (the file name prefix `sat_` / `unsat_` is the
  expected answer)
- pigeonhole 3→2 up to 8→7
- n-queens for n = 1..12 (UNSAT exactly for n = 2 and 3)
- CDCL vs DPLL agreement on 150 random 3-SAT instances in the hard region, with every
  model verified
- larger random instances
- known Sudoku puzzles, including Arto Inkala's "hardest Sudoku", with their published
  solutions and uniqueness checks
- the CLI's exit codes

Debug builds use smaller random instances. Run `cargo test --release` for the full sizes.

## Status / limitations

Learning project. On a laptop it solves random 3-SAT with 200 variables at the
threshold in well under a second, but it is roughly an order of magnitude slower per
conflict than MiniSat. It lacks recursive minimization, LBD-based clause management,
preprocessing and inprocessing. There are no incremental assumptions and no proof
output (DRAT), so UNSAT answers are not independently certified (only SAT models are
checked).

## License

MIT, see [LICENSE](LICENSE).
