//! Sudoku as SAT: variable (r, c, d) = "cell (r, c) holds digit d+1".

use crate::cnf::{Cnf, Lit};
use crate::SolveResult;

pub type Grid = [[u8; 9]; 9];

fn var(r: usize, c: usize, d: usize) -> usize {
    r * 81 + c * 9 + d
}

/// Parse 81 cells; digits 1-9, and `0` or `.` for blanks. Whitespace is ignored.
pub fn parse(s: &str) -> Result<Grid, String> {
    let cells: Vec<u8> = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '|' && *c != '-' && *c != '+')
        .map(|c| match c {
            '1'..='9' => Ok(c as u8 - b'0'),
            '0' | '.' | '_' => Ok(0),
            _ => Err(format!("unexpected character '{c}'")),
        })
        .collect::<Result<_, _>>()?;
    if cells.len() != 81 {
        return Err(format!("expected 81 cells, got {}", cells.len()));
    }
    let mut g = [[0; 9]; 9];
    for (i, v) in cells.into_iter().enumerate() {
        g[i / 9][i % 9] = v;
    }
    Ok(g)
}

pub fn format(g: &Grid) -> String {
    let mut s = String::new();
    for (r, row) in g.iter().enumerate() {
        if r > 0 && r % 3 == 0 {
            s.push_str("------+-------+------\n");
        }
        for (c, v) in row.iter().enumerate() {
            if c > 0 && c % 3 == 0 {
                s.push_str("| ");
            }
            s.push(if *v == 0 { '.' } else { (b'0' + v) as char });
            s.push(' ');
        }
        s.pop();
        s.push('\n');
    }
    s
}

/// Encode the rules plus the givens as CNF (729 variables).
pub fn encode(g: &Grid) -> Cnf {
    let mut cnf = Cnf::new(729);
    let groups: Vec<Vec<(usize, usize)>> = (0..9)
        .map(|r| (0..9).map(|c| (r, c)).collect())
        .chain((0..9).map(|c| (0..9).map(|r| (r, c)).collect()))
        .chain((0..9).map(|b| (0..9).map(|i| (b / 3 * 3 + i / 3, b % 3 * 3 + i % 3)).collect()))
        .collect();
    for r in 0..9 {
        for c in 0..9 {
            // Exactly one digit per cell.
            cnf.add((0..9).map(|d| Lit::new(var(r, c, d), true)));
            for d1 in 0..9 {
                for d2 in d1 + 1..9 {
                    cnf.add([Lit::new(var(r, c, d1), false), Lit::new(var(r, c, d2), false)]);
                }
            }
        }
    }
    for group in &groups {
        for d in 0..9 {
            // Each digit at least once and at most once per row/column/box.
            cnf.add(group.iter().map(|&(r, c)| Lit::new(var(r, c, d), true)));
            for i in 0..9 {
                for j in i + 1..9 {
                    let (a, b) = (group[i], group[j]);
                    cnf.add([Lit::new(var(a.0, a.1, d), false), Lit::new(var(b.0, b.1, d), false)]);
                }
            }
        }
    }
    for (r, row) in g.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            if v != 0 {
                cnf.add([Lit::new(var(r, c, v as usize - 1), true)]);
            }
        }
    }
    cnf
}

pub fn decode(model: &[bool]) -> Grid {
    let mut g = [[0; 9]; 9];
    for (r, row) in g.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = (0..9).find(|&d| model[var(r, c, d)]).map_or(0, |d| d as u8 + 1);
        }
    }
    g
}

/// Is `g` a complete, valid solution that agrees with `puzzle`'s givens?
pub fn is_valid_solution(puzzle: &Grid, g: &Grid) -> bool {
    let ok_set = |cells: Vec<u8>| {
        let mut s = cells;
        s.sort();
        s == [1, 2, 3, 4, 5, 6, 7, 8, 9]
    };
    (0..9).all(|i| {
        ok_set(g[i].to_vec())
            && ok_set((0..9).map(|r| g[r][i]).collect())
            && ok_set((0..9).map(|k| g[i / 3 * 3 + k / 3][i % 3 * 3 + k % 3]).collect())
    }) && (0..81).all(|i| puzzle[i / 9][i % 9] == 0 || puzzle[i / 9][i % 9] == g[i / 9][i % 9])
}

pub fn solve(puzzle: &Grid) -> Option<Grid> {
    match crate::cdcl::solve(&encode(puzzle)) {
        SolveResult::Sat(m) => Some(decode(&m)),
        SolveResult::Unsat => None,
    }
}

/// Is the solution unique? Solve, then forbid that solution and solve again.
pub fn is_unique(puzzle: &Grid) -> Option<bool> {
    let mut cnf = encode(puzzle);
    let SolveResult::Sat(m) = crate::cdcl::solve(&cnf) else { return None };
    let sol = decode(&m);
    cnf.add((0..81).map(|i| Lit::new(var(i / 9, i % 9, sol[i / 9][i % 9] as usize - 1), false)));
    Some(!crate::cdcl::solve(&cnf).is_sat())
}
