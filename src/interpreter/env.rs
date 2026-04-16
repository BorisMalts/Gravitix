use std::collections::HashMap;
use crate::value::Value;

// ─────────────────────────────────────────────────────────────────────────────
// Environment: a simple Vec<Frame> stack for O(1) push/pop
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Frame {
    vars: HashMap<String, Value>,
}

impl Frame {
    fn new() -> Self { Self { vars: HashMap::new() } }
}

#[derive(Clone)]
pub struct Env {
    frames: Vec<Frame>,
    /// Deferred blocks to run when function returns (Feature 4)
    pub defers: Vec<Vec<crate::ast::Stmt>>,
}

impl Env {
    pub fn new() -> Self { Self { frames: vec![Frame::new()], defers: Vec::new() } }

    /// Push a deferred block (Feature 4)
    pub fn push_defer(&mut self, body: Vec<crate::ast::Stmt>) {
        self.defers.push(body);
    }

    /// Take all deferred blocks (drain in reverse order)
    pub fn take_defers(&mut self) -> Vec<Vec<crate::ast::Stmt>> {
        let mut d = std::mem::take(&mut self.defers);
        d.reverse();
        d
    }

    pub fn push(&mut self) { self.frames.push(Frame::new()); }

    pub fn pop(&mut self) { if self.frames.len() > 1 { self.frames.pop(); } }

    pub fn get(&self, name: &str) -> Option<Value> {
        for frame in self.frames.iter().rev() {
            if let Some(v) = frame.vars.get(name) { return Some(v.clone()); }
        }
        None
    }

    pub fn set(&mut self, name: &str, val: Value) {
        // update existing binding first (any frame)
        for frame in self.frames.iter_mut().rev() {
            if frame.vars.contains_key(name) {
                frame.vars.insert(name.to_string(), val);
                return;
            }
        }
        // otherwise declare in current frame
        self.frames.last_mut().unwrap().vars.insert(name.to_string(), val);
    }

    pub fn define(&mut self, name: &str, val: Value) {
        self.frames.last_mut().unwrap().vars.insert(name.to_string(), val);
    }

    /// Return all visible variable names (for "did you mean?" suggestions)
    pub fn all_var_names(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for frame in self.frames.iter().rev() {
            for k in frame.vars.keys() {
                if seen.insert(k.clone()) {
                    result.push(k.clone());
                }
            }
        }
        result
    }

    /// Return all visible variables (for debug/breakpoint)
    pub fn all_vars(&self) -> Vec<(String, Value)> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for frame in self.frames.iter().rev() {
            for (k, v) in &frame.vars {
                if seen.insert(k.clone()) {
                    result.push((k.clone(), v.clone()));
                }
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }
}
