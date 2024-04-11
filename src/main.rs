use sat_solver::cnf::Cnf;
use sat_solver::{cdcl, dpll, gen, sudoku, SolveResult};
use std::process::ExitCode;
use std::time::Instant;

const USAGE: &str = "usage:
  sat solve [--dpll] <file.cnf | ->   solve a DIMACS file (exit 10 = SAT, 20 = UNSAT)
  sat sudoku <81 chars | file>          solve a Sudoku ('.' or 0 for blanks)
  sat gen php <pigeons> <holes>         print a pigeonhole instance
  sat gen random <vars> <clauses> [seed] print a random 3-SAT instance
  sat gen queens <n>                    print an n-queens instance";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        ["solve", rest @ ..] => {
            let (use_dpll, path) = match rest {
                ["--dpll", p] => (true, *p),
                [p] => (false, *p),
                _ => return usage(),
            };
            let text = if path == "-" {
                std::io::read_to_string(std::io::stdin()).unwrap_or_default()
            } else {
                match std::fs::read_to_string(path) {
                    Ok(t) => t,
                    Err(e) => return fail(&format!("{path}: {e}")),
                }
            };
            let cnf = match Cnf::parse_dimacs(&text) {
                Ok(c) => c,
                Err(e) => return fail(&e.to_string()),
            };
            let t = Instant::now();
            let result = if use_dpll {
                dpll::solve(&cnf)
            } else {
                let mut s = cdcl::Solver::new(&cnf);
                let r = s.solve();
                let st = s.stats;
                println!(
                    "c conflicts {} decisions {} propagations {} restarts {} learnt {} deleted {}",
                    st.conflicts, st.decisions, st.propagations, st.restarts, st.learnt, st.deleted
                );
                r
            };
            println!("c solved in {:.3}s", t.elapsed().as_secs_f64());
            match result {
                SolveResult::Sat(model) => {
                    assert!(cnf.is_satisfied_by(&model), "internal error: model does not satisfy formula");
                    println!("s SATISFIABLE");
                    let lits: Vec<String> = model
                        .iter()
                        .enumerate()
                        .map(|(v, &b)| if b { format!("{}", v + 1) } else { format!("-{}", v + 1) })
                        .collect();
                    for chunk in lits.chunks(20) {
                        println!("v {}", chunk.join(" "));
                    }
                    println!("v 0");
                    ExitCode::from(10)
                }
                SolveResult::Unsat => {
                    println!("s UNSATISFIABLE");
                    ExitCode::from(20)
                }
            }
        }
        ["sudoku", src] => {
            let text = std::fs::read_to_string(src).unwrap_or_else(|_| src.to_string());
            let puzzle = match sudoku::parse(&text) {
                Ok(p) => p,
                Err(e) => return fail(&e),
            };
            let t = Instant::now();
            match sudoku::solve(&puzzle) {
                Some(g) => {
                    print!("{}", sudoku::format(&g));
                    println!("solved in {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
                    ExitCode::SUCCESS
                }
                None => fail("no solution"),
            }
        }
        ["gen", "php", p, h] => emit(gen::pigeonhole(p.parse().unwrap_or(5), h.parse().unwrap_or(4))),
        ["gen", "random", v, c, rest @ ..] => {
            let seed = rest.first().and_then(|s| s.parse().ok()).unwrap_or(1);
            emit(gen::random_ksat(v.parse().unwrap_or(50), c.parse().unwrap_or(213), 3, seed))
        }
        ["gen", "queens", n] => emit(gen::queens(n.parse().unwrap_or(8))),
        _ => usage(),
    }
}

fn emit(cnf: Cnf) -> ExitCode {
    print!("{}", cnf.to_dimacs());
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("sat: {msg}");
    ExitCode::from(1)
}
