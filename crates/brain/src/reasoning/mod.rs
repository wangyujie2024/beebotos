//! Reasoning Engine
//!
//! Deductive, inductive, and abductive reasoning.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub mod deductive;

/// Knowledge base
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBase {
    facts: Vec<Fact>,
    rules: Vec<Rule>,
    index: HashMap<String, Vec<usize>>, // Predicate -> rule indices
}

impl KnowledgeBase {
    pub fn new() -> Self {
        Self {
            facts: vec![],
            rules: vec![],
            index: HashMap::new(),
        }
    }

    /// Add fact
    pub fn add_fact(&mut self, fact: Fact) {
        self.facts.push(fact);
    }

    /// Add rule
    pub fn add_rule(&mut self, rule: Rule) {
        let idx = self.rules.len();
        self.rules.push(rule.clone());

        // Index by head predicate
        self.index
            .entry(rule.head.predicate.clone())
            .or_default()
            .push(idx);
    }

    /// Query facts
    pub fn query(&self, predicate: &str) -> Vec<&Fact> {
        self.facts
            .iter()
            .filter(|f| f.predicate == predicate)
            .collect()
    }

    /// Forward chaining inference
    pub fn forward_chain(&mut self, max_iterations: usize) -> Vec<Fact> {
        let mut new_facts = vec![];
        let mut changed = true;
        let mut iteration = 0;

        while changed && iteration < max_iterations {
            changed = false;
            iteration += 1;

            for rule in &self.rules {
                if let Some(instances) = self.match_rule(rule) {
                    for bindings in instances {
                        let new_fact = self.instantiate(&rule.head, &bindings);
                        if !self.facts.contains(&new_fact) {
                            self.facts.push(new_fact.clone());
                            new_facts.push(new_fact);
                            changed = true;
                        }
                    }
                }
            }
        }

        new_facts
    }

    /// Backward chaining
    pub fn prove(&self, goal: &Atom) -> Option<Vec<Binding>> {
        let mut proofs = vec![];
        self.backward_chain(goal, &mut Binding::new(), &mut proofs)?;

        if proofs.is_empty() {
            None
        } else {
            Some(proofs)
        }
    }

    fn backward_chain(
        &self,
        goal: &Atom,
        bindings: &mut Binding,
        proofs: &mut Vec<Binding>,
    ) -> Option<()> {
        // Check if goal matches a fact
        for fact in &self.facts {
            if let Some(new_bindings) = self.unify_atom(
                goal,
                &Atom {
                    predicate: fact.predicate.clone(),
                    args: fact.args.clone(),
                },
                bindings,
            ) {
                proofs.push(new_bindings);
                return Some(());
            }
        }

        // Check rules that can derive this goal
        if let Some(rule_indices) = self.index.get(&goal.predicate) {
            for &idx in rule_indices {
                let rule = &self.rules[idx];
                if let Some(rule_bindings) = self.unify_atom(goal, &rule.head, bindings) {
                    // Prove all body atoms
                    let mut body_bindings = rule_bindings;
                    let mut all_proved = true;

                    for body_atom in &rule.body {
                        if self
                            .backward_chain(body_atom, &mut body_bindings, proofs)
                            .is_none()
                        {
                            all_proved = false;
                            break;
                        }
                    }

                    if all_proved {
                        return Some(());
                    }
                }
            }
        }

        None
    }

    fn match_rule(&self, _rule: &Rule) -> Option<Vec<Binding>> {
        // Simplified matching - would need proper unification in production
        Some(vec![])
    }

    fn instantiate(&self, atom: &Atom, bindings: &Binding) -> Fact {
        Fact {
            predicate: atom.predicate.clone(),
            args: atom
                .args
                .iter()
                .map(|a| {
                    if let Term::Variable(v) = a {
                        bindings.get(v).cloned().unwrap_or_else(|| a.clone())
                    } else {
                        a.clone()
                    }
                })
                .collect(),
            confidence: 1.0,
        }
    }

    fn unify_atom(&self, a1: &Atom, a2: &Atom, bindings: &Binding) -> Option<Binding> {
        if a1.predicate != a2.predicate || a1.args.len() != a2.args.len() {
            return None;
        }

        let mut new_bindings = bindings.clone();

        for (t1, t2) in a1.args.iter().zip(&a2.args) {
            if let Some(b) = self.unify_terms(t1, t2, &new_bindings) {
                new_bindings = b;
            } else {
                return None;
            }
        }

        Some(new_bindings)
    }

    fn unify_terms(&self, t1: &Term, t2: &Term, bindings: &Binding) -> Option<Binding> {
        match (t1, t2) {
            (Term::Constant(c1), Term::Constant(c2)) if c1 == c2 => Some(bindings.clone()),
            (Term::Variable(v), t) | (t, Term::Variable(v)) => {
                let mut new = bindings.clone();
                new.insert(v.clone(), t.clone());
                Some(new)
            }
            _ => None,
        }
    }
}

impl Default for KnowledgeBase {
    fn default() -> Self {
        Self::new()
    }
}

/// Fact
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fact {
    pub predicate: String,
    pub args: Vec<Term>,
    pub confidence: f32,
}

impl Fact {
    pub fn new(predicate: impl Into<String>) -> Self {
        Self {
            predicate: predicate.into(),
            args: vec![],
            confidence: 1.0,
        }
    }

    pub fn with_arg(mut self, arg: Term) -> Self {
        self.args.push(arg);
        self
    }

    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }
}

/// Rule
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rule {
    pub head: Atom,
    pub body: Vec<Atom>,
    pub confidence: f32,
}

impl Rule {
    pub fn new(head: Atom) -> Self {
        Self {
            head,
            body: vec![],
            confidence: 1.0,
        }
    }

    pub fn if_(mut self, atom: Atom) -> Self {
        self.body.push(atom);
        self
    }

    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }
}

/// Atom (predicate with arguments)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Atom {
    pub predicate: String,
    pub args: Vec<Term>,
}

impl Atom {
    pub fn new(predicate: impl Into<String>) -> Self {
        Self {
            predicate: predicate.into(),
            args: vec![],
        }
    }

    pub fn arg(mut self, term: Term) -> Self {
        self.args.push(term);
        self
    }
}

/// Term
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Term {
    Constant(String),
    Variable(String),
    Number(f64),
}

impl Term {
    pub fn const_(s: impl Into<String>) -> Self {
        Term::Constant(s.into())
    }

    pub fn var(s: impl Into<String>) -> Self {
        Term::Variable(s.into())
    }

    pub fn num(n: f64) -> Self {
        Term::Number(n)
    }
}

/// Variable bindings
pub type Binding = HashMap<String, Term>;

/// Inference result
#[derive(Debug, Clone)]
pub struct InferenceResult {
    pub conclusion: Fact,
    pub confidence: f32,
    pub proof_tree: ProofNode,
}

/// Proof tree node
#[derive(Debug, Clone)]
pub enum ProofNode {
    Fact(Fact),
    RuleApplication {
        rule: Rule,
        sub_proofs: Vec<ProofNode>,
    },
}
