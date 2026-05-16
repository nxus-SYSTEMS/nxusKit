//! Bayesian Network graph structure.
//!
//! Provides the core `BayesianNetwork` struct for constructing and querying
//! discrete Bayesian Networks with validated CPTs.

use std::collections::HashMap;

use petgraph::algo::is_cyclic_directed;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::Topo;

use super::error::{BayesError, BayesResult};
use super::factor::Factor;
#[allow(unused_imports)]
use super::types::{DiscreteVariable, GaussianVariable, StateName, VariableName};

/// A Bayesian Network represented as a DAG with CPTs.
///
/// Supports both discrete and continuous (Gaussian) variables.
/// Mixed networks follow the Conditional Linear Gaussian (CLG) model:
/// discrete variables can be parents of continuous variables, but
/// continuous variables MUST NOT be parents of discrete variables.
#[derive(Debug, Clone)]
pub struct BayesianNetwork {
    /// The directed acyclic graph structure.
    graph: DiGraph<VariableName, ()>,
    /// Map from variable name to node index.
    name_to_index: HashMap<VariableName, NodeIndex>,
    /// Discrete variable definitions (name → discrete variable with states).
    variables: HashMap<VariableName, DiscreteVariable>,
    /// Gaussian variable definitions (name → continuous variable).
    gaussian_variables: HashMap<VariableName, GaussianVariable>,
    /// Conditional probability tables stored as factors (variable name → CPT factor).
    cpts: HashMap<VariableName, Factor>,
    /// Canonical parent ordering per variable (in the order edges were added).
    /// This is the ordering used for CPT indexing — do NOT rely on petgraph's
    /// `neighbors_directed` for CPT-related operations.
    parent_order: HashMap<VariableName, Vec<VariableName>>,
}

impl Default for BayesianNetwork {
    fn default() -> Self {
        Self::new()
    }
}

impl BayesianNetwork {
    /// Create a new, empty Bayesian Network.
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            name_to_index: HashMap::new(),
            variables: HashMap::new(),
            gaussian_variables: HashMap::new(),
            cpts: HashMap::new(),
            parent_order: HashMap::new(),
        }
    }

    /// Add a discrete variable to the network.
    ///
    /// # Errors
    /// Returns an error if a variable with the same name already exists.
    pub fn add_variable(&mut self, var: DiscreteVariable) -> BayesResult<()> {
        if self.name_to_index.contains_key(&var.name) {
            return Err(BayesError::InvalidGraph(format!(
                "Variable '{}' already exists",
                var.name
            )));
        }
        let idx = self.graph.add_node(var.name.clone());
        self.name_to_index.insert(var.name.clone(), idx);
        self.variables.insert(var.name.clone(), var);
        Ok(())
    }

    /// Add a continuous Gaussian variable to the network.
    ///
    /// # Errors
    /// Returns an error if a variable with the same name already exists.
    pub fn add_gaussian_variable(&mut self, var: GaussianVariable) -> BayesResult<()> {
        if self.name_to_index.contains_key(&var.name) {
            return Err(BayesError::InvalidGraph(format!(
                "Variable '{}' already exists",
                var.name
            )));
        }
        let idx = self.graph.add_node(var.name.clone());
        self.name_to_index.insert(var.name.clone(), idx);
        self.gaussian_variables.insert(var.name.clone(), var);
        Ok(())
    }

    /// Update an existing Gaussian variable (e.g., to add weights).
    /// Returns an error if the variable doesn't exist or isn't Gaussian.
    pub fn update_gaussian_variable(&mut self, var: GaussianVariable) -> BayesResult<()> {
        if !self.gaussian_variables.contains_key(&var.name) {
            return Err(BayesError::InvalidGraph(format!(
                "Gaussian variable '{}' does not exist",
                var.name
            )));
        }
        self.gaussian_variables.insert(var.name.clone(), var);
        Ok(())
    }

    /// Check if a variable name corresponds to a Gaussian variable.
    pub fn is_gaussian(&self, name: &VariableName) -> bool {
        self.gaussian_variables.contains_key(name)
    }

    /// Check if a variable name corresponds to a discrete variable.
    pub fn is_discrete(&self, name: &VariableName) -> bool {
        self.variables.contains_key(name)
    }

    /// Get a Gaussian variable by name.
    pub fn gaussian_variable(&self, name: &VariableName) -> Option<&GaussianVariable> {
        self.gaussian_variables.get(name)
    }

    /// Get all Gaussian variables.
    pub fn gaussian_variables(&self) -> &HashMap<VariableName, GaussianVariable> {
        &self.gaussian_variables
    }

    /// Check if the network has any Gaussian (continuous) variables.
    pub fn has_continuous_variables(&self) -> bool {
        !self.gaussian_variables.is_empty()
    }

    /// Add a directed edge from `parent` to `child`.
    ///
    /// # Errors
    /// Returns an error if either variable doesn't exist or the edge would create a cycle.
    pub fn add_edge(&mut self, parent: &VariableName, child: &VariableName) -> BayesResult<()> {
        let &parent_idx = self.name_to_index.get(parent).ok_or_else(|| {
            BayesError::InvalidGraph(format!("Parent variable '{}' not found", parent))
        })?;
        let &child_idx = self.name_to_index.get(child).ok_or_else(|| {
            BayesError::InvalidGraph(format!("Child variable '{}' not found", child))
        })?;

        // CLG constraint: continuous variables MUST NOT be parents of discrete variables
        if self.is_gaussian(parent) && self.is_discrete(child) {
            return Err(BayesError::CLGViolation(format!(
                "Continuous variable '{}' cannot be a parent of discrete variable '{}' (CLG constraint: discrete→continuous only)",
                parent, child
            )));
        }

        // Add edge tentatively
        let edge_idx = self.graph.add_edge(parent_idx, child_idx, ());

        // Check for cycles
        if is_cyclic_directed(&self.graph) {
            self.graph.remove_edge(edge_idx);
            return Err(BayesError::InvalidGraph(format!(
                "Edge '{}' → '{}' would create a cycle",
                parent, child
            )));
        }

        // Track canonical parent ordering (insertion order)
        self.parent_order
            .entry(child.clone())
            .or_default()
            .push(parent.clone());

        Ok(())
    }

    /// Remove a directed edge from `parent` to `child`.
    ///
    /// # Errors
    /// Returns an error if either variable doesn't exist or the edge doesn't exist.
    pub fn remove_edge(&mut self, parent: &VariableName, child: &VariableName) -> BayesResult<()> {
        let &parent_idx = self.name_to_index.get(parent).ok_or_else(|| {
            BayesError::InvalidGraph(format!("Parent variable '{}' not found", parent))
        })?;
        let &child_idx = self.name_to_index.get(child).ok_or_else(|| {
            BayesError::InvalidGraph(format!("Child variable '{}' not found", child))
        })?;

        // Find and remove the edge
        let edge = self.graph.find_edge(parent_idx, child_idx).ok_or_else(|| {
            BayesError::InvalidGraph(format!("Edge '{}' → '{}' does not exist", parent, child))
        })?;
        self.graph.remove_edge(edge);

        // Remove from parent_order
        if let Some(parents) = self.parent_order.get_mut(child) {
            parents.retain(|p| p != parent);
        }

        Ok(())
    }

    /// Set the conditional probability table for a variable.
    ///
    /// The CPT must have dimensions matching [parent1_card, parent2_card, ..., var_card]
    /// in the order the parents were added (topological parent order in the graph).
    ///
    /// # Errors
    /// Returns an error if the variable doesn't exist, dimensions don't match,
    /// or probabilities don't form valid distributions.
    pub fn set_cpt(&mut self, variable: &VariableName, probabilities: Vec<f64>) -> BayesResult<()> {
        let var = self.variables.get(variable).ok_or_else(|| {
            BayesError::InvalidGraph(format!("Variable '{}' not found", variable))
        })?;

        let parents = self.parents(variable);
        let mut factor_vars = Vec::new();
        let mut factor_cards = Vec::new();

        for parent_name in &parents {
            let parent_var = &self.variables[parent_name];
            factor_vars.push(parent_name.clone());
            factor_cards.push(parent_var.cardinality());
        }
        factor_vars.push(variable.clone());
        factor_cards.push(var.cardinality());

        let expected_size: usize = factor_cards.iter().product();
        if probabilities.len() != expected_size {
            return Err(BayesError::InvalidCpt(format!(
                "Variable '{}': expected {} CPT entries (parents {:?} × {} states), got {}",
                variable,
                expected_size,
                parents.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
                var.cardinality(),
                probabilities.len()
            )));
        }

        // Validate that each parent configuration row sums to ~1.0
        let child_card = var.cardinality();
        let num_parent_configs = expected_size / child_card;
        for config_idx in 0..num_parent_configs {
            let row_start = config_idx * child_card;
            let row_sum: f64 = probabilities[row_start..row_start + child_card]
                .iter()
                .sum();
            if (row_sum - 1.0).abs() > 1e-6 {
                return Err(BayesError::InvalidCpt(format!(
                    "Variable '{}': CPT row {} sums to {} (expected ~1.0)",
                    variable, config_idx, row_sum
                )));
            }
        }

        let factor = Factor::from_probabilities(factor_vars, factor_cards, probabilities)?;
        self.cpts.insert(variable.clone(), factor);
        Ok(())
    }

    /// Get the parents of a variable in canonical (insertion) order.
    ///
    /// This ordering is consistent with CPT indexing and must be used
    /// for all CPT-related operations.
    pub fn parents(&self, variable: &VariableName) -> Vec<VariableName> {
        self.parent_order.get(variable).cloned().unwrap_or_default()
    }

    /// Get the children of a variable.
    pub fn children(&self, variable: &VariableName) -> Vec<VariableName> {
        let Some(&node_idx) = self.name_to_index.get(variable) else {
            return vec![];
        };
        self.graph
            .neighbors_directed(node_idx, petgraph::Direction::Outgoing)
            .map(|idx| self.graph[idx].clone())
            .collect()
    }

    /// Return variable names in topological order.
    pub fn topological_sort(&self) -> Vec<VariableName> {
        let mut topo = Topo::new(&self.graph);
        let mut result = Vec::new();
        while let Some(idx) = topo.next(&self.graph) {
            result.push(self.graph[idx].clone());
        }
        result
    }

    /// Get all variable names (discrete + continuous).
    pub fn variable_names(&self) -> Vec<VariableName> {
        self.variables
            .keys()
            .chain(self.gaussian_variables.keys())
            .cloned()
            .collect()
    }

    /// Get a variable by name.
    pub fn variable(&self, name: &VariableName) -> Option<&DiscreteVariable> {
        self.variables.get(name)
    }

    /// Get a variable's CPT factor.
    pub fn cpt(&self, name: &VariableName) -> Option<&Factor> {
        self.cpts.get(name)
    }

    /// Number of variables (discrete + continuous).
    pub fn num_variables(&self) -> usize {
        self.variables.len() + self.gaussian_variables.len()
    }

    /// Number of edges.
    pub fn num_edges(&self) -> usize {
        self.graph.edge_count()
    }

    /// Check if the network is complete (all discrete variables have CPTs;
    /// Gaussian variables are self-describing).
    pub fn is_complete(&self) -> bool {
        self.variables
            .keys()
            .all(|name| self.cpts.contains_key(name))
    }

    /// Validate that the network is complete and all CPTs are well-formed.
    /// Gaussian variables are validated at construction time.
    pub fn validate(&self) -> BayesResult<()> {
        for name in self.variables.keys() {
            if !self.cpts.contains_key(name) {
                return Err(BayesError::IncompleteNetwork(format!(
                    "Variable '{}' has no CPT",
                    name
                )));
            }
        }
        Ok(())
    }

    /// Get the Markov blanket of a variable: parents + children + children's other parents.
    pub fn markov_blanket(&self, variable: &VariableName) -> Vec<VariableName> {
        let mut blanket = std::collections::HashSet::new();

        // Parents
        for parent in self.parents(variable) {
            blanket.insert(parent);
        }

        // Children and children's other parents
        for child in self.children(variable) {
            blanket.insert(child.clone());
            for co_parent in self.parents(&child) {
                if &co_parent != variable {
                    blanket.insert(co_parent);
                }
            }
        }

        blanket.into_iter().collect()
    }

    /// Get all variables as a map.
    pub fn variables(&self) -> &HashMap<VariableName, DiscreteVariable> {
        &self.variables
    }

    /// Get all CPTs as a map.
    pub fn cpts(&self) -> &HashMap<VariableName, Factor> {
        &self.cpts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var_name(s: &str) -> VariableName {
        VariableName::new(s).unwrap()
    }

    fn state(s: &str) -> StateName {
        StateName::new(s).unwrap()
    }

    fn binary_var(name: &str) -> DiscreteVariable {
        DiscreteVariable::new(var_name(name), vec![state("yes"), state("no")]).unwrap()
    }

    #[test]
    fn add_variable_and_query() {
        let mut net = BayesianNetwork::new();
        net.add_variable(binary_var("A")).unwrap();
        assert_eq!(net.num_variables(), 1);
        assert!(net.variable(&var_name("A")).is_some());
        assert_eq!(net.variable(&var_name("A")).unwrap().cardinality(), 2);
    }

    #[test]
    fn add_duplicate_variable_fails() {
        let mut net = BayesianNetwork::new();
        net.add_variable(binary_var("A")).unwrap();
        assert!(net.add_variable(binary_var("A")).is_err());
    }

    #[test]
    fn add_edge_and_query_parents_children() {
        let mut net = BayesianNetwork::new();
        net.add_variable(binary_var("A")).unwrap();
        net.add_variable(binary_var("B")).unwrap();
        net.add_edge(&var_name("A"), &var_name("B")).unwrap();

        assert_eq!(net.num_edges(), 1);
        assert_eq!(net.parents(&var_name("B")), vec![var_name("A")]);
        assert_eq!(net.children(&var_name("A")), vec![var_name("B")]);
    }

    #[test]
    fn cycle_detection() {
        let mut net = BayesianNetwork::new();
        net.add_variable(binary_var("A")).unwrap();
        net.add_variable(binary_var("B")).unwrap();
        net.add_edge(&var_name("A"), &var_name("B")).unwrap();
        let result = net.add_edge(&var_name("B"), &var_name("A"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn add_edge_unknown_variable_fails() {
        let mut net = BayesianNetwork::new();
        net.add_variable(binary_var("A")).unwrap();
        assert!(net.add_edge(&var_name("A"), &var_name("Z")).is_err());
        assert!(net.add_edge(&var_name("Z"), &var_name("A")).is_err());
    }

    #[test]
    fn set_cpt_root_node() {
        let mut net = BayesianNetwork::new();
        net.add_variable(binary_var("A")).unwrap();
        net.set_cpt(&var_name("A"), vec![0.3, 0.7]).unwrap();
        assert!(net.cpt(&var_name("A")).is_some());
    }

    #[test]
    fn set_cpt_with_parent() {
        let mut net = BayesianNetwork::new();
        net.add_variable(binary_var("A")).unwrap();
        net.add_variable(binary_var("B")).unwrap();
        net.add_edge(&var_name("A"), &var_name("B")).unwrap();
        // P(B|A): 2 parent configs × 2 child states = 4 entries
        net.set_cpt(&var_name("B"), vec![0.2, 0.8, 0.9, 0.1])
            .unwrap();
    }

    #[test]
    fn set_cpt_wrong_size_fails() {
        let mut net = BayesianNetwork::new();
        net.add_variable(binary_var("A")).unwrap();
        assert!(net.set_cpt(&var_name("A"), vec![0.5]).is_err());
    }

    #[test]
    fn set_cpt_row_not_summing_to_one_fails() {
        let mut net = BayesianNetwork::new();
        net.add_variable(binary_var("A")).unwrap();
        assert!(net.set_cpt(&var_name("A"), vec![0.3, 0.3]).is_err());
    }

    #[test]
    fn topological_sort() {
        let mut net = BayesianNetwork::new();
        net.add_variable(binary_var("C")).unwrap();
        net.add_variable(binary_var("A")).unwrap();
        net.add_variable(binary_var("B")).unwrap();
        net.add_edge(&var_name("A"), &var_name("B")).unwrap();
        net.add_edge(&var_name("B"), &var_name("C")).unwrap();

        let order = net.topological_sort();
        let a_pos = order.iter().position(|v| v == &var_name("A")).unwrap();
        let b_pos = order.iter().position(|v| v == &var_name("B")).unwrap();
        let c_pos = order.iter().position(|v| v == &var_name("C")).unwrap();
        assert!(a_pos < b_pos);
        assert!(b_pos < c_pos);
    }

    #[test]
    fn validate_complete_network() {
        let mut net = BayesianNetwork::new();
        net.add_variable(binary_var("A")).unwrap();
        net.set_cpt(&var_name("A"), vec![0.5, 0.5]).unwrap();
        assert!(net.validate().is_ok());
        assert!(net.is_complete());
    }

    #[test]
    fn validate_incomplete_network() {
        let mut net = BayesianNetwork::new();
        net.add_variable(binary_var("A")).unwrap();
        assert!(net.validate().is_err());
        assert!(!net.is_complete());
    }

    // === Gaussian variable and CLG constraint tests (Part 2) ===

    #[test]
    fn add_gaussian_variable() {
        let mut net = BayesianNetwork::new();
        let gv = super::super::types::GaussianVariable::new("Temperature", 20.0, 5.0).unwrap();
        net.add_gaussian_variable(gv).unwrap();
        assert_eq!(net.num_variables(), 1);
        assert!(net.is_gaussian(&var_name("Temperature")));
        assert!(!net.is_discrete(&var_name("Temperature")));
        assert!(net.gaussian_variable(&var_name("Temperature")).is_some());
        assert!(net.has_continuous_variables());
    }

    #[test]
    fn add_duplicate_gaussian_variable_fails() {
        let mut net = BayesianNetwork::new();
        let gv = super::super::types::GaussianVariable::new("Temperature", 20.0, 5.0).unwrap();
        net.add_gaussian_variable(gv).unwrap();
        let gv2 = super::super::types::GaussianVariable::new("Temperature", 10.0, 2.0).unwrap();
        assert!(net.add_gaussian_variable(gv2).is_err());
    }

    #[test]
    fn clg_discrete_parent_of_continuous_allowed() {
        let mut net = BayesianNetwork::new();
        net.add_variable(binary_var("Mode")).unwrap();
        let gv = super::super::types::GaussianVariable::new("Sensor", 0.0, 1.0).unwrap();
        net.add_gaussian_variable(gv).unwrap();
        // Discrete → Continuous is allowed
        assert!(net.add_edge(&var_name("Mode"), &var_name("Sensor")).is_ok());
    }

    #[test]
    fn clg_continuous_parent_of_discrete_rejected() {
        let mut net = BayesianNetwork::new();
        let gv = super::super::types::GaussianVariable::new("Sensor", 0.0, 1.0).unwrap();
        net.add_gaussian_variable(gv).unwrap();
        net.add_variable(binary_var("Alarm")).unwrap();
        // Continuous → Discrete is rejected (CLG constraint)
        let result = net.add_edge(&var_name("Sensor"), &var_name("Alarm"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("CLG"));
    }

    #[test]
    fn clg_continuous_parent_of_continuous_allowed() {
        let mut net = BayesianNetwork::new();
        let gv1 = super::super::types::GaussianVariable::new("Temp", 20.0, 5.0).unwrap();
        let gv2 = super::super::types::GaussianVariable::new("Sensor", 0.0, 1.0).unwrap();
        net.add_gaussian_variable(gv1).unwrap();
        net.add_gaussian_variable(gv2).unwrap();
        // Continuous → Continuous is allowed
        assert!(net.add_edge(&var_name("Temp"), &var_name("Sensor")).is_ok());
    }

    #[test]
    fn mixed_network_variable_names() {
        let mut net = BayesianNetwork::new();
        net.add_variable(binary_var("A")).unwrap();
        let gv = super::super::types::GaussianVariable::new("X", 0.0, 1.0).unwrap();
        net.add_gaussian_variable(gv).unwrap();
        let names = net.variable_names();
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn markov_blanket() {
        // A → B → C, A → C
        let mut net = BayesianNetwork::new();
        net.add_variable(binary_var("A")).unwrap();
        net.add_variable(binary_var("B")).unwrap();
        net.add_variable(binary_var("C")).unwrap();
        net.add_edge(&var_name("A"), &var_name("B")).unwrap();
        net.add_edge(&var_name("B"), &var_name("C")).unwrap();
        net.add_edge(&var_name("A"), &var_name("C")).unwrap();

        let blanket = net.markov_blanket(&var_name("B"));
        // B's Markov blanket: parent A, child C, C's other parent A (already included)
        assert!(blanket.contains(&var_name("A")));
        assert!(blanket.contains(&var_name("C")));
        assert!(!blanket.contains(&var_name("B")));
    }
}
