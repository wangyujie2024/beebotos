//! Deductive Reasoning Engine
//!
//! Implements logical deduction from premises to conclusions.
//! Supports syllogistic reasoning, modus ponens, and chain reasoning.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// A logical statement
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Statement {
    /// Atomic proposition
    Atom(String),
    /// Negation
    Not(Box<Statement>),
    /// Conjunction (AND)
    And(Box<Statement>, Box<Statement>),
    /// Disjunction (OR)
    Or(Box<Statement>, Box<Statement>),
    /// Implication (IF-THEN)
    Implies(Box<Statement>, Box<Statement>),
    /// Universal quantification
    ForAll(String, Box<Statement>),
    /// Existential quantification
    Exists(String, Box<Statement>),
}

impl Statement {
    /// Create atomic statement
    pub fn atom(s: impl Into<String>) -> Self {
        Statement::Atom(s.into())
    }

    /// Create negation
    pub fn negated(self) -> Self {
        Statement::Not(Box::new(self))
    }

    /// Create conjunction
    pub fn and(self, other: Statement) -> Self {
        Statement::And(Box::new(self), Box::new(other))
    }

    /// Create disjunction
    pub fn or(self, other: Statement) -> Self {
        Statement::Or(Box::new(self), Box::new(other))
    }

    /// Create implication
    pub fn implies(self, consequent: Statement) -> Self {
        Statement::Implies(Box::new(self), Box::new(consequent))
    }

    /// Check if statement contains a variable
    pub fn contains(&self, var: &str) -> bool {
        match self {
            Statement::Atom(s) => s.contains(var),
            Statement::Not(s) => s.contains(var),
            Statement::And(a, b) => a.contains(var) || b.contains(var),
            Statement::Or(a, b) => a.contains(var) || b.contains(var),
            Statement::Implies(a, b) => a.contains(var) || b.contains(var),
            Statement::ForAll(v, s) => v == var || s.contains(var),
            Statement::Exists(v, s) => v == var || s.contains(var),
        }
    }
}

/// A deductive rule
#[derive(Debug, Clone)]
pub struct Rule {
    pub premises: Vec<Statement>,
    pub conclusion: Statement,
    pub confidence: f32, // 0.0 to 1.0
}

impl Rule {
    /// Create a new rule
    pub fn new(premises: Vec<Statement>, conclusion: Statement) -> Self {
        Self {
            premises,
            conclusion,
            confidence: 1.0,
        }
    }

    /// Set confidence level
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
}

/// Knowledge base for deductive reasoning
pub struct KnowledgeBase {
    facts: HashSet<Statement>,
    rules: Vec<Rule>,
    derivations: HashMap<Statement, Vec<DerivationStep>>,
}

#[derive(Debug, Clone)]
pub struct DerivationStep {
    pub statement: Statement,
    pub rule_applied: Option<usize>, // Index of rule
    pub premises_used: Vec<Statement>,
}

impl KnowledgeBase {
    /// Create empty knowledge base
    pub fn new() -> Self {
        Self {
            facts: HashSet::new(),
            rules: Vec::new(),
            derivations: HashMap::new(),
        }
    }

    /// Add a fact
    pub fn add_fact(&mut self, fact: Statement) {
        self.facts.insert(fact);
    }

    /// Add multiple facts
    pub fn add_facts(&mut self, facts: Vec<Statement>) {
        for fact in facts {
            self.facts.insert(fact);
        }
    }

    /// Add a rule
    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.push(rule);
    }

    /// Check if statement is known (fact or derived)
    pub fn knows(&self, statement: &Statement) -> bool {
        self.facts.contains(statement) || self.derivations.contains_key(statement)
    }

    /// Forward chaining inference
    pub fn infer_forward(&mut self, max_iterations: usize) -> Vec<Statement> {
        let mut new_facts = Vec::new();
        let mut changed = true;
        let mut iterations = 0;

        while changed && iterations < max_iterations {
            changed = false;
            iterations += 1;

            for (rule_idx, rule) in self.rules.iter().enumerate() {
                // Check if all premises are satisfied and conclusion is not known
                if self.premises_satisfied(&rule.premises) && !self.knows(&rule.conclusion) {
                    let conclusion = rule.conclusion.clone();
                    self.derivations.insert(
                        conclusion.clone(),
                        vec![DerivationStep {
                            statement: conclusion.clone(),
                            rule_applied: Some(rule_idx),
                            premises_used: rule.premises.clone(),
                        }],
                    );
                    new_facts.push(conclusion);
                    changed = true;
                }
            }
        }

        new_facts
    }

    /// Backward chaining - prove a goal
    pub fn prove(&self, goal: &Statement, max_depth: usize) -> Option<Proof> {
        self.prove_recursive(goal, max_depth, 0, &mut HashSet::new())
    }

    fn prove_recursive(
        &self,
        goal: &Statement,
        max_depth: usize,
        current_depth: usize,
        visited: &mut HashSet<Statement>,
    ) -> Option<Proof> {
        if current_depth > max_depth {
            return None;
        }

        // Check if it's a known fact
        if self.facts.contains(goal) {
            return Some(Proof {
                goal: goal.clone(),
                steps: vec![ProofStep::Fact(goal.clone())],
                valid: true,
            });
        }

        // Check if already derived
        if let Some(derivation) = self.derivations.get(goal) {
            return Some(Proof {
                goal: goal.clone(),
                steps: vec![ProofStep::Derived(goal.clone(), derivation.clone())],
                valid: true,
            });
        }

        // Avoid cycles
        if visited.contains(goal) {
            return None;
        }
        visited.insert(goal.clone());

        // Try to find a rule that concludes this goal
        for (rule_idx, rule) in self.rules.iter().enumerate() {
            if Self::statements_equal(&rule.conclusion, goal) {
                // Try to prove all premises
                let mut premise_proofs = Vec::new();
                let mut all_proven = true;

                for premise in &rule.premises {
                    if let Some(proof) = self.prove_recursive(
                        premise,
                        max_depth,
                        current_depth + 1,
                        &mut visited.clone(),
                    ) {
                        premise_proofs.push(proof);
                    } else {
                        all_proven = false;
                        break;
                    }
                }

                if all_proven {
                    return Some(Proof {
                        goal: goal.clone(),
                        steps: vec![ProofStep::RuleApplication {
                            rule_idx,
                            premises: premise_proofs,
                            conclusion: goal.clone(),
                        }],
                        valid: true,
                    });
                }
            }
        }

        None
    }

    /// Check if premises are all satisfied
    fn premises_satisfied(&self, premises: &[Statement]) -> bool {
        premises.iter().all(|p| self.knows(p))
    }

    /// Compare statements (simplified)
    fn statements_equal(a: &Statement, b: &Statement) -> bool {
        format!("{:?}", a) == format!("{:?}", b)
    }

    /// Get all known facts
    pub fn facts(&self) -> &HashSet<Statement> {
        &self.facts
    }

    /// Get all rules
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }
}

impl Default for KnowledgeBase {
    fn default() -> Self {
        Self::new()
    }
}

/// A proof result
#[derive(Debug, Clone)]
pub struct Proof {
    pub goal: Statement,
    pub steps: Vec<ProofStep>,
    pub valid: bool,
}

/// Individual proof step
#[derive(Debug, Clone)]
pub enum ProofStep {
    Fact(Statement),
    Derived(Statement, Vec<DerivationStep>),
    RuleApplication {
        rule_idx: usize,
        premises: Vec<Proof>,
        conclusion: Statement,
    },
}

/// Deductive reasoning engine
pub struct DeductiveEngine {
    kb: KnowledgeBase,
}

impl DeductiveEngine {
    /// Create new engine
    pub fn new() -> Self {
        Self {
            kb: KnowledgeBase::new(),
        }
    }

    /// Get mutable knowledge base
    pub fn kb_mut(&mut self) -> &mut KnowledgeBase {
        &mut self.kb
    }

    /// Get knowledge base
    pub fn kb(&self) -> &KnowledgeBase {
        &self.kb
    }

    /// Apply modus ponens: if P and P->Q, then Q
    pub fn modus_ponens(&mut self) -> Vec<Statement> {
        let mut conclusions = Vec::new();

        // Collect rules that can be applied (to avoid borrow issues)
        let applicable_rules: Vec<Statement> = self
            .kb
            .rules()
            .iter()
            .filter(|rule| {
                // Check if all premises are satisfied
                let all_premises_known = rule.premises.iter().all(|p| self.kb.knows(p));
                all_premises_known && !self.kb.knows(&rule.conclusion)
            })
            .map(|rule| rule.conclusion.clone())
            .collect();

        for conclusion in applicable_rules {
            self.kb.add_fact(conclusion.clone());
            conclusions.push(conclusion);
        }

        conclusions
    }

    /// Chain reasoning: follow implications
    pub fn chain_reasoning(&self, start: &Statement) -> Vec<Vec<Statement>> {
        let mut chains = Vec::new();
        let mut current_chain = vec![start.clone()];

        self.chain_recursive(start, &mut current_chain, &mut chains, 10);

        chains
    }

    fn chain_recursive(
        &self,
        current: &Statement,
        chain: &mut Vec<Statement>,
        chains: &mut Vec<Vec<Statement>>,
        max_depth: usize,
    ) {
        if max_depth == 0 {
            if chain.len() > 1 {
                chains.push(chain.clone());
            }
            return;
        }

        // Find rules where current is a premise
        for rule in self.kb.rules() {
            for premise in &rule.premises {
                if Self::statement_matches(premise, current) {
                    chain.push(rule.conclusion.clone());
                    self.chain_recursive(&rule.conclusion, chain, chains, max_depth - 1);
                    chain.pop();
                }
            }
        }
    }

    fn statement_matches(a: &Statement, b: &Statement) -> bool {
        format!("{:?}", a) == format!("{:?}", b)
    }
}

impl Default for DeductiveEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_knowledge_base_facts() {
        let mut kb = KnowledgeBase::new();
        kb.add_fact(Statement::atom("Socrates is mortal"));

        assert!(kb.knows(&Statement::atom("Socrates is mortal")));
        assert!(!kb.knows(&Statement::atom("All men are mortal")));
    }

    #[test]
    fn test_syllogism() {
        let mut kb = KnowledgeBase::new();

        // All men are mortal
        kb.add_fact(Statement::atom("All men are mortal"));
        // Socrates is a man
        kb.add_fact(Statement::atom("Socrates is a man"));

        // If X is a man, then X is mortal
        kb.add_rule(Rule::new(
            vec![Statement::atom("Socrates is a man")],
            Statement::atom("Socrates is mortal"),
        ));

        let _new_facts = kb.infer_forward(10);

        assert!(kb.knows(&Statement::atom("Socrates is mortal")));
    }

    #[test]
    fn test_modus_ponens() {
        let mut engine = DeductiveEngine::new();

        // P: It is raining
        engine.kb_mut().add_fact(Statement::atom("It is raining"));

        // P -> Q: If it is raining, then the ground is wet
        engine.kb_mut().add_rule(Rule::new(
            vec![Statement::atom("It is raining")],
            Statement::atom("The ground is wet"),
        ));

        let _conclusions = engine.modus_ponens();

        assert!(engine.kb().knows(&Statement::atom("The ground is wet")));
    }
}
