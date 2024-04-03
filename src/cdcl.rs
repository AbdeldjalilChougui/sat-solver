//! Conflict-driven clause learning (MiniSat-style):
//! two watched literals, 1-UIP learning with clause minimization,
//! non-chronological backjumping, VSIDS with a binary heap, phase saving,
//! Luby restarts and periodic deletion of inactive learnt clauses.

use crate::cnf::{Cnf, Lit};
use crate::SolveResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LBool {
    True,
    False,
    Undef,
}

struct Clause {
    lits: Vec<Lit>,
    learnt: bool,
    activity: f64,
    deleted: bool,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub decisions: u64,
    pub propagations: u64,
    pub conflicts: u64,
    pub restarts: u64,
    pub learnt: u64,
    pub deleted: u64,
}

/// Max-heap of variables keyed by activity, with position index for decrease/increase-key.
struct VarHeap {
    heap: Vec<usize>,
    pos: Vec<Option<usize>>,
}

impl VarHeap {
    fn new(n: usize) -> VarHeap {
        let mut h = VarHeap { heap: Vec::with_capacity(n), pos: vec![None; n] };
        for v in 0..n {
            h.heap.push(v);
            h.pos[v] = Some(v);
        }
        h
    }

    fn contains(&self, v: usize) -> bool {
        self.pos[v].is_some()
    }

    fn swap(&mut self, i: usize, j: usize) {
        self.heap.swap(i, j);
        self.pos[self.heap[i]] = Some(i);
        self.pos[self.heap[j]] = Some(j);
    }

    fn up(&mut self, mut i: usize, act: &[f64]) {
        while i > 0 {
            let p = (i - 1) / 2;
            if act[self.heap[i]] <= act[self.heap[p]] {
                break;
            }
            self.swap(i, p);
            i = p;
        }
    }

    fn down(&mut self, mut i: usize, act: &[f64]) {
        loop {
            let (l, r) = (2 * i + 1, 2 * i + 2);
            let mut big = i;
            if l < self.heap.len() && act[self.heap[l]] > act[self.heap[big]] {
                big = l;
            }
            if r < self.heap.len() && act[self.heap[r]] > act[self.heap[big]] {
                big = r;
            }
            if big == i {
                return;
            }
            self.swap(i, big);
            i = big;
        }
    }

    fn insert(&mut self, v: usize, act: &[f64]) {
        if self.contains(v) {
            return;
        }
        self.heap.push(v);
        let i = self.heap.len() - 1;
        self.pos[v] = Some(i);
        self.up(i, act);
    }

    fn bumped(&mut self, v: usize, act: &[f64]) {
        if let Some(i) = self.pos[v] {
            self.up(i, act);
        }
    }

    fn pop(&mut self, act: &[f64]) -> Option<usize> {
        if self.heap.is_empty() {
            return None;
        }
        let top = self.heap[0];
        let last = self.heap.len() - 1;
        self.swap(0, last);
        self.heap.pop();
        self.pos[top] = None;
        if !self.heap.is_empty() {
            self.down(0, act);
        }
        Some(top)
    }
}

/// A watch-list entry. `blocker` is some other literal of the clause: if it
/// is already true the clause is satisfied and we skip it without touching
/// the clause's memory (MiniSat's "blocking literal" trick).
#[derive(Debug, Clone, Copy)]
struct Watcher {
    ci: usize,
    blocker: Lit,
}

pub struct Solver {
    num_vars: usize,
    clauses: Vec<Clause>,
    /// `watches[l]`: clauses with `l` among their first two literals,
    /// visited when `l` becomes false.
    watches: Vec<Vec<Watcher>>,
    assigns: Vec<LBool>,
    level: Vec<usize>,
    reason: Vec<Option<usize>>,
    trail: Vec<Lit>,
    trail_lim: Vec<usize>,
    qhead: usize,
    activity: Vec<f64>,
    var_inc: f64,
    cla_inc: f64,
    order: VarHeap,
    polarity: Vec<bool>,
    seen: Vec<bool>,
    ok: bool,
    num_learnts: usize,
    num_deleted: usize,
    pub stats: Stats,
}

const VAR_DECAY: f64 = 0.95;
const CLA_DECAY: f64 = 0.999;
const RESTART_BASE: u64 = 100;

/// Luby sequence: 1 1 2 1 1 2 4 1 1 2 1 1 2 4 8 ...
pub fn luby(mut i: u64) -> u64 {
    let (mut size, mut seq) = (1u64, 0u32);
    while size < i + 1 {
        seq += 1;
        size = 2 * size + 1;
    }
    while size - 1 != i {
        size = (size - 1) >> 1;
        seq -= 1;
        i %= size;
    }
    1 << seq
}

impl Solver {
    pub fn new(cnf: &Cnf) -> Solver {
        let n = cnf.num_vars;
        let mut s = Solver {
            num_vars: n,
            clauses: Vec::new(),
            watches: vec![Vec::new(); 2 * n],
            assigns: vec![LBool::Undef; n],
            level: vec![0; n],
            reason: vec![None; n],
            trail: Vec::new(),
            trail_lim: Vec::new(),
            qhead: 0,
            activity: vec![0.0; n],
            var_inc: 1.0,
            cla_inc: 1.0,
            order: VarHeap::new(n),
            polarity: vec![false; n],
            seen: vec![false; n],
            ok: true,
            num_learnts: 0,
            num_deleted: 0,
            stats: Stats::default(),
        };
        for c in &cnf.clauses {
            s.add_clause(c);
            if !s.ok {
                break;
            }
        }
        s
    }

    fn value(&self, l: Lit) -> LBool {
        match self.assigns[l.var()] {
            LBool::Undef => LBool::Undef,
            LBool::True if l.is_positive() => LBool::True,
            LBool::False if !l.is_positive() => LBool::True,
            _ => LBool::False,
        }
    }

    fn decision_level(&self) -> usize {
        self.trail_lim.len()
    }

    fn enqueue(&mut self, l: Lit, reason: Option<usize>) {
        let v = l.var();
        self.assigns[v] = if l.is_positive() { LBool::True } else { LBool::False };
        self.level[v] = self.decision_level();
        self.reason[v] = reason;
        self.trail.push(l);
    }

    /// Add an original clause (only at decision level 0).
    pub fn add_clause(&mut self, lits: &[Lit]) {
        let mut c: Vec<Lit> = lits.to_vec();
        c.sort();
        c.dedup();
        if c.windows(2).any(|w| w[0] == !w[1]) {
            return; // tautology
        }
        // Drop literals already false at level 0; skip if already satisfied.
        if c.iter().any(|&l| self.value(l) == LBool::True) {
            return;
        }
        c.retain(|&l| self.value(l) == LBool::Undef);
        match c.len() {
            0 => self.ok = false,
            1 => {
                self.enqueue(c[0], None);
                if self.propagate().is_some() {
                    self.ok = false;
                }
            }
            _ => {
                self.attach(Clause { lits: c, learnt: false, activity: 0.0, deleted: false });
            }
        }
    }

    fn attach(&mut self, c: Clause) -> usize {
        let ci = self.clauses.len();
        self.watches[c.lits[0].index()].push(Watcher { ci, blocker: c.lits[1] });
        self.watches[c.lits[1].index()].push(Watcher { ci, blocker: c.lits[0] });
        self.clauses.push(c);
        ci
    }

    /// Unit propagation with two watched literals. Returns a conflicting clause.
    fn propagate(&mut self) -> Option<usize> {
        while self.qhead < self.trail.len() {
            let p = self.trail[self.qhead];
            self.qhead += 1;
            self.stats.propagations += 1;
            let false_lit = !p;
            let ws = std::mem::take(&mut self.watches[false_lit.index()]);
            let mut keep = Vec::with_capacity(ws.len());
            let mut conflict = None;
            let mut i = 0;
            while i < ws.len() {
                let w = ws[i];
                i += 1;
                if self.value(w.blocker) == LBool::True {
                    keep.push(w);
                    continue;
                }
                let ci = w.ci;
                if self.clauses[ci].deleted {
                    continue; // lazily drop watches of deleted clauses
                }
                // Make sure the false literal is at position 1.
                {
                    let lits = &mut self.clauses[ci].lits;
                    if lits[0] == false_lit {
                        lits.swap(0, 1);
                    }
                }
                let first = self.clauses[ci].lits[0];
                if self.value(first) == LBool::True {
                    keep.push(Watcher { ci, blocker: first });
                    continue;
                }
                // Look for a new literal to watch.
                let len = self.clauses[ci].lits.len();
                let mut moved = false;
                for k in 2..len {
                    let lk = self.clauses[ci].lits[k];
                    if self.value(lk) != LBool::False {
                        self.clauses[ci].lits.swap(1, k);
                        self.watches[lk.index()].push(Watcher { ci, blocker: first });
                        moved = true;
                        break;
                    }
                }
                if moved {
                    continue;
                }
                keep.push(Watcher { ci, blocker: first });
                if self.value(first) == LBool::False {
                    conflict = Some(ci);
                    keep.extend_from_slice(&ws[i..]);
                    break;
                }
                self.enqueue(first, Some(ci));
            }
            // Watches added during the loop for false_lit itself can't happen
            // (we never move a watch onto a false literal), so just restore.
            let added = std::mem::take(&mut self.watches[false_lit.index()]);
            keep.extend(added);
            self.watches[false_lit.index()] = keep;
            if conflict.is_some() {
                self.qhead = self.trail.len();
                return conflict;
            }
        }
        None
    }

    fn bump_var(&mut self, v: usize) {
        self.activity[v] += self.var_inc;
        if self.activity[v] > 1e100 {
            for a in &mut self.activity {
                *a *= 1e-100;
            }
            self.var_inc *= 1e-100;
        }
        self.order.bumped(v, &self.activity);
    }

    fn bump_clause(&mut self, ci: usize) {
        if !self.clauses[ci].learnt {
            return;
        }
        self.clauses[ci].activity += self.cla_inc;
        if self.clauses[ci].activity > 1e20 {
            for c in self.clauses.iter_mut().filter(|c| c.learnt) {
                c.activity *= 1e-20;
            }
            self.cla_inc *= 1e-20;
        }
    }

    /// 1-UIP conflict analysis. Returns the learnt clause (asserting literal
    /// first, a literal of the backjump level second) and the backjump level.
    fn analyze(&mut self, mut confl: usize) -> (Vec<Lit>, usize) {
        let mut learnt = vec![Lit(0)]; // placeholder for the UIP
        let mut counter = 0;
        let mut p: Option<Lit> = None;
        let mut idx = self.trail.len();
        loop {
            self.bump_clause(confl);
            let start = usize::from(p.is_some());
            for j in start..self.clauses[confl].lits.len() {
                let q = self.clauses[confl].lits[j];
                let v = q.var();
                if !self.seen[v] && self.level[v] > 0 {
                    self.seen[v] = true;
                    self.bump_var(v);
                    if self.level[v] >= self.decision_level() {
                        counter += 1;
                    } else {
                        learnt.push(q);
                    }
                }
            }
            // Walk back to the next marked literal on the trail.
            loop {
                idx -= 1;
                if self.seen[self.trail[idx].var()] {
                    break;
                }
            }
            let lit = self.trail[idx];
            p = Some(lit);
            self.seen[lit.var()] = false;
            counter -= 1;
            if counter == 0 {
                break;
            }
            confl = self.reason[lit.var()].expect("implied literal has a reason");
        }
        learnt[0] = !p.unwrap();

        // Minimization: drop literals whose reason is entirely implied by the rest.
        let marked: Vec<usize> = learnt[1..].iter().map(|l| l.var()).collect();
        let mut out = vec![learnt[0]];
        for &l in &learnt[1..] {
            let redundant = match self.reason[l.var()] {
                None => false,
                Some(r) => self.clauses[r].lits[1..].iter().all(|q| self.seen[q.var()] || self.level[q.var()] == 0),
            };
            if !redundant {
                out.push(l);
            }
        }
        for v in marked {
            self.seen[v] = false;
        }

        let bt = if out.len() == 1 {
            0
        } else {
            let mut max_i = 1;
            for i in 2..out.len() {
                if self.level[out[i].var()] > self.level[out[max_i].var()] {
                    max_i = i;
                }
            }
            out.swap(1, max_i);
            self.level[out[1].var()]
        };
        (out, bt)
    }

    fn cancel_until(&mut self, level: usize) {
        if self.decision_level() <= level {
            return;
        }
        let lim = self.trail_lim[level];
        for i in (lim..self.trail.len()).rev() {
            let l = self.trail[i];
            let v = l.var();
            self.polarity[v] = l.is_positive();
            self.assigns[v] = LBool::Undef;
            self.reason[v] = None;
            self.order.insert(v, &self.activity);
        }
        self.trail.truncate(lim);
        self.trail_lim.truncate(level);
        self.qhead = self.trail.len();
    }

    fn pick_branch(&mut self) -> Option<Lit> {
        while let Some(v) = self.order.pop(&self.activity) {
            if self.assigns[v] == LBool::Undef {
                return Some(Lit::new(v, self.polarity[v]));
            }
        }
        None
    }

    fn is_locked(&self, ci: usize) -> bool {
        let l = self.clauses[ci].lits[0];
        self.value(l) == LBool::True && self.reason[l.var()] == Some(ci)
    }

    /// Delete the less active half of the learnt clauses (keeping binary and locked ones).
    fn reduce_db(&mut self) {
        let mut learnts: Vec<usize> =
            (0..self.clauses.len()).filter(|&i| self.clauses[i].learnt && !self.clauses[i].deleted).collect();
        learnts.sort_by(|&a, &b| self.clauses[a].activity.total_cmp(&self.clauses[b].activity));
        let half = learnts.len() / 2;
        for &ci in &learnts[..half] {
            if self.clauses[ci].lits.len() > 2 && !self.is_locked(ci) {
                self.clauses[ci].deleted = true;
                self.clauses[ci].lits.shrink_to_fit();
                self.num_learnts -= 1;
                self.stats.deleted += 1;
                self.num_deleted += 1;
            }
        }
        if self.num_deleted * 2 > self.clauses.len() {
            self.collect_garbage();
        }
    }

    /// Drop deleted clauses from the database, renumbering the survivors and
    /// rebuilding reasons and watch lists. Keeps every pass over the clause
    /// database proportional to the number of live clauses.
    fn collect_garbage(&mut self) {
        let mut remap = vec![usize::MAX; self.clauses.len()];
        let old = std::mem::take(&mut self.clauses);
        for (i, c) in old.into_iter().enumerate() {
            if !c.deleted {
                remap[i] = self.clauses.len();
                self.clauses.push(c);
            }
        }
        for r in self.reason.iter_mut().flatten() {
            *r = remap[*r];
            debug_assert!(*r != usize::MAX, "a reason clause was deleted");
        }
        for w in &mut self.watches {
            w.retain_mut(|x| {
                x.ci = remap[x.ci];
                x.ci != usize::MAX
            });
        }
        self.num_deleted = 0;
    }

    pub fn solve(&mut self) -> SolveResult {
        if !self.ok {
            return SolveResult::Unsat;
        }
        let mut restart_i = 0;
        let mut max_learnts = (self.clauses.len() / 3).max(1000) as f64;
        loop {
            let budget = luby(restart_i) * RESTART_BASE;
            let mut conflicts_here = 0;
            loop {
                if let Some(confl) = self.propagate() {
                    self.stats.conflicts += 1;
                    conflicts_here += 1;
                    if self.decision_level() == 0 {
                        return SolveResult::Unsat;
                    }
                    let (learnt, bt) = self.analyze(confl);
                    self.cancel_until(bt);
                    if learnt.len() == 1 {
                        self.enqueue(learnt[0], None);
                    } else {
                        let ci = self.attach(Clause { lits: learnt, learnt: true, activity: 0.0, deleted: false });
                        self.bump_clause(ci);
                        self.num_learnts += 1;
                        self.stats.learnt += 1;
                        let l0 = self.clauses[ci].lits[0];
                        self.enqueue(l0, Some(ci));
                    }
                    self.var_inc /= VAR_DECAY;
                    self.cla_inc /= CLA_DECAY;
                } else {
                    if conflicts_here >= budget {
                        self.stats.restarts += 1;
                        self.cancel_until(0);
                        break;
                    }
                    if self.num_learnts as f64 >= max_learnts + self.trail.len() as f64 {
                        self.reduce_db();
                        max_learnts *= 1.1;
                    }
                    match self.pick_branch() {
                        None => {
                            let model = self.assigns.iter().map(|&a| a == LBool::True).collect();
                            return SolveResult::Sat(model);
                        }
                        Some(l) => {
                            self.stats.decisions += 1;
                            self.trail_lim.push(self.trail.len());
                            self.enqueue(l, None);
                        }
                    }
                }
            }
            restart_i += 1;
        }
    }

    pub fn num_vars(&self) -> usize {
        self.num_vars
    }
}

pub fn solve(cnf: &Cnf) -> SolveResult {
    Solver::new(cnf).solve()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luby_sequence() {
        let got: Vec<u64> = (0..15).map(luby).collect();
        assert_eq!(got, vec![1, 1, 2, 1, 1, 2, 4, 1, 1, 2, 1, 1, 2, 4, 8]);
    }

    #[test]
    fn heap_pops_by_activity() {
        let act = vec![0.5, 3.0, 1.0, 2.0];
        let mut h = VarHeap::new(4);
        for i in (0..2).rev() {
            h.down(i, &act);
        }
        let order: Vec<usize> = std::iter::from_fn(|| h.pop(&act)).collect();
        assert_eq!(order, vec![1, 3, 2, 0]);
    }
}
