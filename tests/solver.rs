use sat_solver::cnf::Cnf;
use sat_solver::{cdcl, dpll, gen, sudoku, SolveResult};

fn check(cnf: &Cnf, r: &SolveResult) {
    if let SolveResult::Sat(m) = r {
        assert!(cnf.is_satisfied_by(m), "model does not satisfy the formula");
    }
}

#[test]
fn known_instances_from_files() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cnf");
    let mut n = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let want_sat = if name.starts_with("sat_") {
            true
        } else if name.starts_with("unsat_") {
            false
        } else {
            continue;
        };
        let cnf = Cnf::parse_dimacs(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let r = cdcl::solve(&cnf);
        assert_eq!(r.is_sat(), want_sat, "CDCL on {name}");
        check(&cnf, &r);
        if cnf.num_vars <= 100 && !name.contains("php") {
            let r = dpll::solve(&cnf);
            assert_eq!(r.is_sat(), want_sat, "DPLL on {name}");
            check(&cnf, &r);
        }
        n += 1;
    }
    assert!(n >= 8);
}

#[test]
fn pigeonhole_is_unsat_and_fits_when_enough_holes() {
    for n in 2..=7 {
        assert_eq!(cdcl::solve(&gen::pigeonhole(n + 1, n)), SolveResult::Unsat, "php {}->{}", n + 1, n);
        let fits = gen::pigeonhole(n, n);
        let r = cdcl::solve(&fits);
        assert!(r.is_sat());
        check(&fits, &r);
    }
    assert_eq!(dpll::solve(&gen::pigeonhole(5, 4)), SolveResult::Unsat);
}

#[test]
fn queens() {
    for n in 1..=12 {
        let cnf = gen::queens(n);
        let r = cdcl::solve(&cnf);
        assert_eq!(r.is_sat(), !(n == 2 || n == 3), "{n}-queens");
        check(&cnf, &r);
    }
}

#[test]
fn cdcl_agrees_with_dpll_on_random_3sat() {
    // Ratio ~4.26 is the hard region where about half the instances are SAT.
    let (mut sat, mut unsat) = (0, 0);
    for seed in 1..=150u64 {
        let cnf = gen::random_ksat(30, 128, 3, seed);
        let a = cdcl::solve(&cnf);
        let b = dpll::solve(&cnf);
        assert_eq!(a.is_sat(), b.is_sat(), "seed {seed}");
        check(&cnf, &a);
        check(&cnf, &b);
        if a.is_sat() {
            sat += 1
        } else {
            unsat += 1
        }
    }
    assert!(sat > 10 && unsat > 10, "{sat} sat / {unsat} unsat: generator not in the hard region?");
}

#[test]
fn larger_random_instances() {
    // Ratio 4.26 near the phase transition: fine for CDCL, hopeless for naive DPLL.
    // Debug builds use a smaller size to keep `cargo test` quick.
    let n = if cfg!(debug_assertions) { 120 } else { 200 };
    for seed in 1..=5u64 {
        let cnf = gen::random_ksat(n, n * 426 / 100, 3, seed);
        check(&cnf, &cdcl::solve(&cnf));
    }
}

const EASY: &str = "53..7....6..195....98....6.8...6...34..8.3..17...2...6.6....28....419..5....8..79";
const EASY_SOLUTION: &str = "534678912672195348198342567859761423426853791713924856961537284287419635345286179";
/// Arto Inkala's "world's hardest sudoku" (2012).
const INKALA: &str = "8..........36......7..9.2...5...7.......457.....1...3...1....68..85...1..9....4..";
const INKALA_SOLUTION: &str = "812753649943682175675491283154237896369845721287169534521974368438526917796318452";

#[test]
fn sudoku_known_puzzles() {
    for (p, s) in [(EASY, EASY_SOLUTION), (INKALA, INKALA_SOLUTION)] {
        let puzzle = sudoku::parse(p).unwrap();
        let got = sudoku::solve(&puzzle).expect("solvable");
        assert!(sudoku::is_valid_solution(&puzzle, &got));
        assert_eq!(got, sudoku::parse(s).unwrap());
        assert_eq!(sudoku::is_unique(&puzzle), Some(true));
    }
}

#[test]
fn sudoku_edge_cases() {
    // Empty grid: many solutions, any valid one is fine.
    let empty = sudoku::parse(&".".repeat(81)).unwrap();
    let g = sudoku::solve(&empty).unwrap();
    assert!(sudoku::is_valid_solution(&empty, &g));
    assert_eq!(sudoku::is_unique(&empty), Some(false));
    // Contradictory givens: two 5s in the first row.
    let bad = format!("55{}", ".".repeat(79));
    assert_eq!(sudoku::solve(&sudoku::parse(&bad).unwrap()), None);
    assert!(sudoku::parse("123").is_err());
}

#[test]
fn cli_exit_codes() {
    let bin = env!("CARGO_BIN_EXE_sat");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cnf");
    let run = |f: &str| std::process::Command::new(bin).arg("solve").arg(dir.join(f)).output().unwrap();
    let out = run("sat_dimacs_example.cnf");
    assert_eq!(out.status.code(), Some(10));
    assert!(String::from_utf8_lossy(&out.stdout).contains("s SATISFIABLE"));
    assert_eq!(run("unsat_chain.cnf").status.code(), Some(20));
    let out = std::process::Command::new(bin).args(["sudoku", EASY]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("5 3 4 | 6 7 8 | 9 1 2"));
}
