//! Literals, CNF formulas and DIMACS I/O.

use std::fmt;

/// A literal: variable index `v` (0-based) encoded as `2v` (positive) or `2v+1` (negative).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Lit(pub u32);

impl Lit {
    pub fn new(var: usize, positive: bool) -> Lit {
        Lit((var as u32) << 1 | (!positive) as u32)
    }

    /// From a DIMACS integer (`3` → x3, `-3` → ¬x3; variables are 1-based there).
    pub fn from_dimacs(n: i64) -> Lit {
        assert!(n != 0);
        Lit::new((n.unsigned_abs() - 1) as usize, n > 0)
    }

    pub fn to_dimacs(self) -> i64 {
        let v = self.var() as i64 + 1;
        if self.is_positive() {
            v
        } else {
            -v
        }
    }

    pub fn var(self) -> usize {
        (self.0 >> 1) as usize
    }

    pub fn is_positive(self) -> bool {
        self.0 & 1 == 0
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl std::ops::Not for Lit {
    type Output = Lit;
    fn not(self) -> Lit {
        Lit(self.0 ^ 1)
    }
}

impl fmt::Display for Lit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_dimacs())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cnf {
    pub num_vars: usize,
    pub clauses: Vec<Vec<Lit>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DIMACS line {}: {}", self.line, self.msg)
    }
}

impl std::error::Error for ParseError {}

impl Cnf {
    pub fn new(num_vars: usize) -> Cnf {
        Cnf { num_vars, clauses: Vec::new() }
    }

    pub fn add(&mut self, clause: impl IntoIterator<Item = Lit>) {
        let c: Vec<Lit> = clause.into_iter().collect();
        for l in &c {
            self.num_vars = self.num_vars.max(l.var() + 1);
        }
        self.clauses.push(c);
    }

    /// Does `model` (indexed by variable) satisfy every clause?
    pub fn is_satisfied_by(&self, model: &[bool]) -> bool {
        self.clauses.iter().all(|c| c.iter().any(|l| model.get(l.var()).copied() == Some(l.is_positive())))
    }

    pub fn parse_dimacs(text: &str) -> Result<Cnf, ParseError> {
        let mut cnf = Cnf::default();
        let mut declared: Option<(usize, usize)> = None;
        let mut current = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let err = |msg: &str| ParseError { line: i + 1, msg: msg.into() };
            let line = line.trim();
            if line.is_empty() || line.starts_with('c') {
                continue;
            }
            if line.starts_with('%') {
                break; // SATLIB end marker
            }
            if let Some(rest) = line.strip_prefix('p') {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                let ["cnf", v, c] = parts.as_slice() else { return Err(err("expected 'p cnf <vars> <clauses>'")) };
                let v = v.parse().map_err(|_| err("bad variable count"))?;
                let c = c.parse().map_err(|_| err("bad clause count"))?;
                declared = Some((v, c));
                cnf.num_vars = v;
                continue;
            }
            if declared.is_none() {
                return Err(err("clause before the 'p cnf' header"));
            }
            for tok in line.split_whitespace() {
                let n: i64 = tok.parse().map_err(|_| err(&format!("bad literal '{tok}'")))?;
                if n == 0 {
                    cnf.clauses.push(std::mem::take(&mut current));
                } else {
                    let lit = Lit::from_dimacs(n);
                    if lit.var() >= cnf.num_vars {
                        return Err(err(&format!("literal {n} exceeds declared variable count")));
                    }
                    current.push(lit);
                }
            }
        }
        if !current.is_empty() {
            cnf.clauses.push(current); // tolerate a missing final 0
        }
        if declared.is_none() {
            return Err(ParseError { line: 0, msg: "missing 'p cnf' header".into() });
        }
        Ok(cnf)
    }

    pub fn to_dimacs(&self) -> String {
        let mut s = format!("p cnf {} {}\n", self.num_vars, self.clauses.len());
        for c in &self.clauses {
            for l in c {
                s.push_str(&format!("{l} "));
            }
            s.push_str("0\n");
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_encoding() {
        let l = Lit::from_dimacs(-3);
        assert_eq!((l.var(), l.is_positive(), l.to_dimacs()), (2, false, -3));
        assert_eq!((!l).to_dimacs(), 3);
    }

    #[test]
    fn parse_and_roundtrip() {
        let text = "c comment\np cnf 3 2\n1 -3 0\n2 3\n-1 0\n%\n0\n";
        let cnf = Cnf::parse_dimacs(text).unwrap();
        assert_eq!(cnf.num_vars, 3);
        assert_eq!(cnf.clauses.len(), 2);
        assert_eq!(cnf.clauses[1].iter().map(|l| l.to_dimacs()).collect::<Vec<_>>(), vec![2, 3, -1]);
        assert_eq!(Cnf::parse_dimacs(&cnf.to_dimacs()).unwrap(), cnf);
    }

    #[test]
    fn parse_errors() {
        assert!(Cnf::parse_dimacs("1 2 0\n").is_err());
        assert!(Cnf::parse_dimacs("p cnf 2 1\n1 5 0\n").is_err());
        assert!(Cnf::parse_dimacs("p cnf 2 1\n1 x 0\n").is_err());
        assert!(Cnf::parse_dimacs("p dnf 2 1\n").is_err());
    }
}
