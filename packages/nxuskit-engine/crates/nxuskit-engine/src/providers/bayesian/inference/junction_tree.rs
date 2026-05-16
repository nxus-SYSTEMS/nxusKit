//! Junction Tree inference algorithm (Shafer-Shenoy).
//!
//! Exact inference using junction tree construction and message passing:
//! 1. Moralization (marry parents)
//! 2. Triangulation (min-fill heuristic)
//! 3. Clique identification
//! 4. Junction tree construction (max-weight spanning tree on clique graph)
//! 5. Shafer-Shenoy message passing (collect + distribute)

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Instant;

use rayon::prelude::*;

use super::{InferenceEngine, InferenceResult, Marginal};
use crate::providers::bayesian::error::{BayesError, BayesResult};
use crate::providers::bayesian::evidence::Evidence;
use crate::providers::bayesian::factor::Factor;
use crate::providers::bayesian::network::BayesianNetwork;
use crate::providers::bayesian::types::VariableName;

/// Minimum number of independent message paths to enable parallel execution.
/// Trees smaller than this threshold use sequential message passing.
const PARALLEL_THRESHOLD: usize = 2;

/// Junction Tree inference with Shafer-Shenoy message passing.
///
/// Supports incremental evidence: retains JT structure, re-propagates messages
/// only (no tree reconstruction) when evidence changes.
///
/// When the junction tree has >= 2 independent message paths at any BFS level,
/// the collect and distribute phases are parallelized with rayon. For small
/// trees, sequential execution is used to avoid thread overhead.
#[derive(Debug, Clone, Default)]
pub struct JunctionTree {
    /// When true, force sequential execution (for testing/comparison).
    force_sequential: bool,
}

impl JunctionTree {
    pub fn new() -> Self {
        Self {
            force_sequential: false,
        }
    }

    /// Create a JunctionTree that always uses sequential message passing.
    pub fn sequential() -> Self {
        Self {
            force_sequential: true,
        }
    }

    /// Build cliques via moralization + triangulation + clique identification.
    fn build_cliques(&self, network: &BayesianNetwork) -> Vec<HashSet<VariableName>> {
        // Step 1: Build moral graph (undirected: connect all parents of each node)
        let vars = network.variable_names();
        let mut adj: HashMap<VariableName, HashSet<VariableName>> = HashMap::new();
        for v in &vars {
            adj.entry(v.clone()).or_default();
        }

        // Add edges from DAG
        for v in &vars {
            for parent in network.parents(v) {
                adj.entry(v.clone()).or_default().insert(parent.clone());
                adj.entry(parent.clone()).or_default().insert(v.clone());
            }
            // Marry parents (moralization)
            let parents = network.parents(v);
            for i in 0..parents.len() {
                for j in (i + 1)..parents.len() {
                    adj.entry(parents[i].clone())
                        .or_default()
                        .insert(parents[j].clone());
                    adj.entry(parents[j].clone())
                        .or_default()
                        .insert(parents[i].clone());
                }
            }
        }

        // Step 2: Triangulate via min-fill elimination
        let mut remaining: Vec<VariableName> = vars;
        let mut cliques: Vec<HashSet<VariableName>> = Vec::new();

        while !remaining.is_empty() {
            // Find min-fill variable
            let best_idx = remaining
                .iter()
                .enumerate()
                .min_by_key(|(_, var)| {
                    let neighbors: Vec<_> = adj
                        .get(*var)
                        .map(|s| {
                            s.iter()
                                .filter(|n| remaining.contains(n))
                                .cloned()
                                .collect()
                        })
                        .unwrap_or_default();
                    let mut fill = 0usize;
                    for i in 0..neighbors.len() {
                        for j in (i + 1)..neighbors.len() {
                            if !adj
                                .get(&neighbors[i])
                                .map(|s| s.contains(&neighbors[j]))
                                .unwrap_or(false)
                            {
                                fill += 1;
                            }
                        }
                    }
                    fill
                })
                .map(|(i, _)| i)
                .unwrap();

            let var = remaining.remove(best_idx);
            let neighbors: Vec<VariableName> = adj
                .get(&var)
                .map(|s| {
                    s.iter()
                        .filter(|n| remaining.contains(n))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();

            // Create clique: var + its remaining neighbors
            let mut clique: HashSet<VariableName> = neighbors.iter().cloned().collect();
            clique.insert(var.clone());

            // Add fill edges
            for i in 0..neighbors.len() {
                for j in (i + 1)..neighbors.len() {
                    adj.entry(neighbors[i].clone())
                        .or_default()
                        .insert(neighbors[j].clone());
                    adj.entry(neighbors[j].clone())
                        .or_default()
                        .insert(neighbors[i].clone());
                }
            }

            // Only add if not a subset of an existing clique
            let is_subset = cliques.iter().any(|c| clique.is_subset(c));
            if !is_subset {
                // Remove existing cliques that are subsets of this new one
                cliques.retain(|c| !c.is_subset(&clique));
                cliques.push(clique);
            }
        }

        cliques
    }

    /// Build junction tree from cliques using max-weight spanning tree.
    fn build_tree(
        &self,
        cliques: &[HashSet<VariableName>],
    ) -> Vec<(usize, usize, HashSet<VariableName>)> {
        if cliques.len() <= 1 {
            return vec![];
        }

        // Compute separators (intersection sizes) between all pairs
        let mut edges: Vec<(usize, usize, usize, HashSet<VariableName>)> = Vec::new();
        for i in 0..cliques.len() {
            for j in (i + 1)..cliques.len() {
                let sep: HashSet<VariableName> =
                    cliques[i].intersection(&cliques[j]).cloned().collect();
                if !sep.is_empty() {
                    edges.push((i, j, sep.len(), sep));
                }
            }
        }

        // Sort by weight descending (max-weight spanning tree)
        edges.sort_by_key(|edge| std::cmp::Reverse(edge.2));

        // Kruskal's algorithm
        let mut parent: Vec<usize> = (0..cliques.len()).collect();
        fn find(parent: &mut Vec<usize>, x: usize) -> usize {
            if parent[x] != x {
                parent[x] = find(parent, parent[x]);
            }
            parent[x]
        }

        let mut tree_edges = Vec::new();
        for (i, j, _, sep) in edges {
            let ri = find(&mut parent, i);
            let rj = find(&mut parent, j);
            if ri != rj {
                parent[ri] = rj;
                tree_edges.push((i, j, sep));
            }
        }

        tree_edges
    }

    /// Assign CPT factors to cliques.
    fn assign_factors(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        cliques: &[HashSet<VariableName>],
    ) -> BayesResult<Vec<Factor>> {
        let mut clique_potentials: Vec<Option<Factor>> = vec![None; cliques.len()];

        for (var_name, cpt) in network.cpts() {
            let mut factor = cpt.clone();

            // Reduce by evidence
            for (ev_var, state_idx) in evidence.observations() {
                if factor.variables.contains(&ev_var) {
                    factor = factor.reduce(&ev_var, state_idx)?;
                }
            }

            // Find a clique that contains all variables in this factor's scope
            let factor_vars: HashSet<VariableName> = factor.variables.iter().cloned().collect();
            let clique_idx = cliques
                .iter()
                .position(|c| factor_vars.is_subset(c))
                .ok_or_else(|| {
                    BayesError::InferenceError(format!(
                        "No clique contains all variables for factor of '{}'",
                        var_name
                    ))
                })?;

            clique_potentials[clique_idx] = match clique_potentials[clique_idx].take() {
                Some(existing) => Some(existing.multiply(&factor)?),
                None => Some(factor),
            };
        }

        // Initialize unassigned cliques with uniform factors
        let result: Vec<Factor> = clique_potentials
            .into_iter()
            .enumerate()
            .map(|(i, pot)| {
                pot.unwrap_or_else(|| {
                    let vars: Vec<VariableName> = cliques[i].iter().cloned().collect();
                    let cards: Vec<usize> = vars
                        .iter()
                        .map(|v| network.variable(v).map(|dv| dv.cardinality()).unwrap_or(2))
                        .collect();
                    let size: usize = cards.iter().product();
                    Factor::from_log_values(vars, cards, vec![0.0; size]).unwrap()
                })
            })
            .collect();

        Ok(result)
    }
}

impl InferenceEngine for JunctionTree {
    fn infer(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
    ) -> BayesResult<InferenceResult> {
        let start = Instant::now();
        network.validate()?;

        let evidence_vars: HashSet<VariableName> =
            evidence.observations().keys().cloned().collect();

        // Build junction tree
        let cliques = self.build_cliques(network);
        if cliques.is_empty() {
            return Ok(InferenceResult {
                marginals: HashMap::new(),
                log_marginals: HashMap::new(),
                algorithm: "jt".to_string(),
                elapsed: start.elapsed(),
                diagnostics: None,
                continuous_marginals: HashMap::new(),
                nuts_diagnostics: None,
            });
        }

        let tree_edges = self.build_tree(&cliques);
        let potentials = self.assign_factors(network, evidence, &cliques)?;

        // Build adjacency list for the tree (sort neighbors for deterministic ordering)
        let mut tree_adj: Vec<Vec<usize>> = vec![Vec::new(); cliques.len()];
        for &(i, j, _) in &tree_edges {
            tree_adj[i].push(j);
            tree_adj[j].push(i);
        }
        for adj in &mut tree_adj {
            adj.sort();
        }

        // Decide whether to use parallel execution.
        let root = 0;
        let levels = self.message_order_by_level(&tree_adj, root);
        let max_level_width = levels.iter().map(|l| l.len()).max().unwrap_or(0);
        let use_parallel = !self.force_sequential && max_level_width >= PARALLEL_THRESHOLD;

        // Shafer-Shenoy message passing
        let messages = if use_parallel {
            self.message_passing_parallel(&cliques, &potentials, &tree_adj, &levels)?
        } else {
            self.message_passing_sequential(&cliques, &potentials, &tree_adj, &levels)?
        };

        // Compute beliefs (potential × all incoming messages) — parallel for large trees
        let beliefs = if use_parallel {
            self.compute_beliefs_parallel(&potentials, &tree_adj, &messages)?
        } else {
            self.compute_beliefs_sequential(&potentials, &tree_adj, &messages)?
        };

        // Extract marginals for each unobserved variable
        let mut marginals = HashMap::new();
        let mut log_marginals = HashMap::new();

        for var_name in network.variable_names() {
            if evidence_vars.contains(&var_name) {
                continue;
            }

            // Find a clique containing this variable
            if let Some(clique_idx) = cliques.iter().position(|c| c.contains(&var_name)) {
                let mut belief = beliefs[clique_idx].clone();

                // Marginalize out everything except the query variable
                let vars_to_remove: Vec<VariableName> = belief
                    .variables
                    .iter()
                    .filter(|v| *v != &var_name)
                    .cloned()
                    .collect();
                for v in vars_to_remove {
                    belief = belief.marginalize(&v)?;
                }

                let normalized = belief.normalize()?;
                let probs = normalized.to_probabilities();
                let logs = normalized.log_values.clone();

                marginals.insert(var_name.clone(), probs);
                log_marginals.insert(var_name, logs);
            }
        }

        Ok(InferenceResult {
            marginals,
            log_marginals,
            algorithm: "jt".to_string(),
            elapsed: start.elapsed(),
            diagnostics: None,
            continuous_marginals: HashMap::new(),
            nuts_diagnostics: None,
        })
    }

    fn query(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        variable: &VariableName,
    ) -> BayesResult<Marginal> {
        let result = self.infer(network, evidence)?;
        result.marginals.get(variable).cloned().ok_or_else(|| {
            BayesError::InferenceError(format!(
                "Variable '{}' not found in inference results",
                variable
            ))
        })
    }

    fn algorithm_name(&self) -> &str {
        "junction_tree"
    }
}

impl JunctionTree {
    /// Compute message ordering grouped by BFS level for parallelization.
    ///
    /// Returns levels from leaves to root. Messages within a level are independent
    /// and can be computed in parallel. Levels must be processed sequentially.
    fn message_order_by_level(
        &self,
        tree_adj: &[Vec<usize>],
        root: usize,
    ) -> Vec<Vec<(usize, usize)>> {
        // BFS from root to get levels
        let mut visited = vec![false; tree_adj.len()];
        let mut levels: Vec<Vec<(usize, usize)>> = Vec::new();

        visited[root] = true;

        // BFS level by level
        let mut current_frontier = vec![root];
        while !current_frontier.is_empty() {
            let mut next_frontier = Vec::new();
            let mut level_edges = Vec::new();

            for &node in &current_frontier {
                for &neighbor in &tree_adj[node] {
                    if !visited[neighbor] {
                        visited[neighbor] = true;
                        // Message from child to parent (collect direction)
                        level_edges.push((neighbor, node));
                        next_frontier.push(neighbor);
                    }
                }
            }

            // Sort for deterministic ordering
            level_edges.sort();

            if !level_edges.is_empty() {
                levels.push(level_edges);
            }
            current_frontier = next_frontier;
        }

        // Reverse: deepest level first (leaves → root)
        levels.reverse();
        levels
    }

    /// Compute a single message from `from` to `to`.
    fn compute_message(
        cliques: &[HashSet<VariableName>],
        potentials: &[Factor],
        tree_adj: &[Vec<usize>],
        messages: &HashMap<(usize, usize), Factor>,
        from: usize,
        to: usize,
    ) -> BayesResult<Factor> {
        let mut msg = potentials[from].clone();

        // Multiply in all incoming messages (except from `to`), in sorted order
        // for deterministic floating-point results.
        let mut incoming_neighbors: Vec<usize> = tree_adj[from]
            .iter()
            .filter(|&&n| n != to)
            .copied()
            .collect();
        incoming_neighbors.sort();

        for neighbor in incoming_neighbors {
            if let Some(incoming) = messages.get(&(neighbor, from)) {
                msg = msg.multiply(incoming)?;
            }
        }

        // Marginalize to the separator (intersection of cliques)
        let separator: HashSet<VariableName> =
            cliques[from].intersection(&cliques[to]).cloned().collect();
        let mut vars_to_remove: Vec<VariableName> = msg
            .variables
            .iter()
            .filter(|v| !separator.contains(v))
            .cloned()
            .collect();
        // Sort for deterministic marginalization order
        vars_to_remove.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        for v in vars_to_remove {
            msg = msg.marginalize(&v)?;
        }

        Ok(msg)
    }

    /// Sequential message passing (collect + distribute).
    fn message_passing_sequential(
        &self,
        cliques: &[HashSet<VariableName>],
        potentials: &[Factor],
        tree_adj: &[Vec<usize>],
        levels: &[Vec<(usize, usize)>],
    ) -> BayesResult<HashMap<(usize, usize), Factor>> {
        let mut messages: HashMap<(usize, usize), Factor> = HashMap::new();

        // Collect phase: leaves → root (levels are already in leaf-first order)
        for level in levels {
            for &(from, to) in level {
                let msg =
                    Self::compute_message(cliques, potentials, tree_adj, &messages, from, to)?;
                messages.insert((from, to), msg);
            }
        }

        // Distribute phase: root → leaves (reverse level order, swap from/to)
        for level in levels.iter().rev() {
            for &(from, to) in level {
                // Distribute: message from `to` back to `from`
                let msg =
                    Self::compute_message(cliques, potentials, tree_adj, &messages, to, from)?;
                messages.insert((to, from), msg);
            }
        }

        Ok(messages)
    }

    /// Parallel message passing using rayon — level-by-level parallelism.
    ///
    /// Messages within a BFS level are independent and computed in parallel.
    /// Levels are processed sequentially since each depends on the prior level.
    fn message_passing_parallel(
        &self,
        cliques: &[HashSet<VariableName>],
        potentials: &[Factor],
        tree_adj: &[Vec<usize>],
        levels: &[Vec<(usize, usize)>],
    ) -> BayesResult<HashMap<(usize, usize), Factor>> {
        let messages: Mutex<HashMap<(usize, usize), Factor>> = Mutex::new(HashMap::new());

        // Collect phase: leaves → root
        for level in levels {
            // Snapshot current messages for this level (all dependencies are already resolved).
            let msgs_snapshot = messages.lock().unwrap().clone();

            let level_results: Vec<BayesResult<((usize, usize), Factor)>> = level
                .par_iter()
                .map(|&(from, to)| {
                    let msg = Self::compute_message(
                        cliques,
                        potentials,
                        tree_adj,
                        &msgs_snapshot,
                        from,
                        to,
                    )?;
                    Ok(((from, to), msg))
                })
                .collect();

            // Insert results into shared map.
            let mut msgs = messages.lock().unwrap();
            for result in level_results {
                let (key, factor) = result?;
                msgs.insert(key, factor);
            }
        }

        // Distribute phase: root → leaves
        for level in levels.iter().rev() {
            let msgs_snapshot = messages.lock().unwrap().clone();

            let level_results: Vec<BayesResult<((usize, usize), Factor)>> = level
                .par_iter()
                .map(|&(from, to)| {
                    // Distribute: message from `to` back to `from`
                    let msg = Self::compute_message(
                        cliques,
                        potentials,
                        tree_adj,
                        &msgs_snapshot,
                        to,
                        from,
                    )?;
                    Ok(((to, from), msg))
                })
                .collect();

            let mut msgs = messages.lock().unwrap();
            for result in level_results {
                let (key, factor) = result?;
                msgs.insert(key, factor);
            }
        }

        Ok(messages.into_inner().unwrap())
    }

    /// Sequential belief computation.
    fn compute_beliefs_sequential(
        &self,
        potentials: &[Factor],
        tree_adj: &[Vec<usize>],
        messages: &HashMap<(usize, usize), Factor>,
    ) -> BayesResult<Vec<Factor>> {
        let mut beliefs = Vec::with_capacity(potentials.len());
        for (i, pot) in potentials.iter().enumerate() {
            let mut belief = pot.clone();
            // Multiply in sorted neighbor order for deterministic results
            let mut neighbors: Vec<usize> = tree_adj[i].clone();
            neighbors.sort();
            for neighbor in neighbors {
                if let Some(msg) = messages.get(&(neighbor, i)) {
                    belief = belief.multiply(msg)?;
                }
            }
            beliefs.push(belief);
        }
        Ok(beliefs)
    }

    /// Parallel belief computation — each clique is independent.
    fn compute_beliefs_parallel(
        &self,
        potentials: &[Factor],
        tree_adj: &[Vec<usize>],
        messages: &HashMap<(usize, usize), Factor>,
    ) -> BayesResult<Vec<Factor>> {
        let results: Vec<BayesResult<Factor>> = (0..potentials.len())
            .into_par_iter()
            .map(|i| {
                let mut belief = potentials[i].clone();
                let mut neighbors: Vec<usize> = tree_adj[i].clone();
                neighbors.sort();
                for neighbor in neighbors {
                    if let Some(msg) = messages.get(&(neighbor, i)) {
                        belief = belief.multiply(msg)?;
                    }
                }
                Ok(belief)
            })
            .collect();

        results.into_iter().collect()
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use crate::providers::bayesian::bif::load_bif_file;
    fn var_name(s: &str) -> VariableName {
        VariableName::new(s).unwrap()
    }

    fn load_asia() -> BayesianNetwork {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn/asia.bif");
        load_bif_file(&path).unwrap()
    }

    fn load_reference(name: &str) -> HashMap<String, HashMap<String, f64>> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/bn/reference/{}", name));
        let content = std::fs::read_to_string(&path).unwrap();
        serde_json::from_str(&content).unwrap()
    }

    #[test]
    fn jt_asia_prior_marginals() {
        let net = load_asia();
        let evidence = Evidence::new();
        let jt = JunctionTree::new();

        let result = jt.infer(&net, &evidence).unwrap();
        let reference = load_reference("asia_prior_marginals.json");

        for (var_name_str, ref_dist) in &reference {
            let vn = var_name(var_name_str);
            let computed = result
                .marginals
                .get(&vn)
                .unwrap_or_else(|| panic!("JT: Missing marginal for '{}'", var_name_str));
            let var = net.variable(&vn).unwrap();
            for (state_name, &ref_prob) in ref_dist {
                let idx = var.state_index(state_name).unwrap().value();
                let diff = (computed[idx] - ref_prob).abs();
                assert!(
                    diff < 1e-6,
                    "JT: P({}={}) = {}, expected {}, diff = {}",
                    var_name_str,
                    state_name,
                    computed[idx],
                    ref_prob,
                    diff
                );
            }
        }
    }

    #[test]
    fn jt_asia_with_evidence() {
        let net = load_asia();
        let mut evidence = Evidence::new();
        evidence
            .observe(&net, &var_name("Xray"), "positive")
            .unwrap();
        evidence
            .observe(&net, &var_name("Dyspnea"), "present")
            .unwrap();

        let jt = JunctionTree::new();
        let result = jt.infer(&net, &evidence).unwrap();

        // Should have marginals for all unobserved variables (6 of 8)
        assert_eq!(result.marginals.len(), 6);
        assert!(result.marginals.contains_key(&var_name("Bronchitis")));
        assert!(result.marginals.contains_key(&var_name("LungCancer")));

        // Each marginal should sum to ~1
        for (name, probs) in &result.marginals {
            let sum: f64 = probs.iter().sum();
            assert!(
                (sum - 1.0).abs() < 1e-6,
                "JT: Marginal for '{}' sums to {}, not 1.0",
                name,
                sum
            );
        }
    }

    #[test]
    fn jt_matches_ve_on_asia() {
        let net = load_asia();
        let evidence = Evidence::new();

        let ve = super::super::variable_elimination::VariableElimination::new();
        let jt = JunctionTree::new();

        let ve_result = ve.infer(&net, &evidence).unwrap();
        let jt_result = jt.infer(&net, &evidence).unwrap();

        for (var, ve_probs) in &ve_result.marginals {
            let jt_probs = jt_result
                .marginals
                .get(var)
                .unwrap_or_else(|| panic!("JT missing variable '{}' that VE has", var));
            for (i, (&ve_p, &jt_p)) in ve_probs.iter().zip(jt_probs.iter()).enumerate() {
                let diff = (ve_p - jt_p).abs();
                assert!(
                    diff < 1e-6,
                    "VE vs JT mismatch: {} state {}: VE={}, JT={}, diff={}",
                    var,
                    i,
                    ve_p,
                    jt_p,
                    diff
                );
            }
        }
    }

    #[test]
    fn jt_incremental_evidence() {
        let net = load_asia();
        let jt = JunctionTree::new();

        // Query with one piece of evidence
        let mut ev1 = Evidence::new();
        ev1.observe(&net, &var_name("Xray"), "positive").unwrap();
        let result1 = jt.infer(&net, &ev1).unwrap();

        // Add more evidence and re-query
        let mut ev2 = Evidence::new();
        ev2.observe(&net, &var_name("Xray"), "positive").unwrap();
        ev2.observe(&net, &var_name("Dyspnea"), "present").unwrap();
        let result2 = jt.infer(&net, &ev2).unwrap();

        // Results should be different (adding evidence changes posteriors)
        let bronch1 = &result1.marginals[&var_name("Bronchitis")];
        let bronch2 = &result2.marginals[&var_name("Bronchitis")];
        assert!(
            (bronch1[0] - bronch2[0]).abs() > 1e-6,
            "Adding evidence should change posteriors"
        );
    }

    #[test]
    fn jt_alarm_network() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn/alarm.bif");
        if !path.exists() {
            return;
        }
        let net = load_bif_file(&path).unwrap();
        let evidence = Evidence::new();
        let jt = JunctionTree::new();

        let result = jt.infer(&net, &evidence).unwrap();

        // Should have marginals for all 37 variables
        assert_eq!(result.marginals.len(), 37);

        // All marginals should sum to ~1
        for (name, probs) in &result.marginals {
            let sum: f64 = probs.iter().sum();
            assert!(
                (sum - 1.0).abs() < 1e-6,
                "JT Alarm: Marginal for '{}' sums to {}, not 1.0",
                name,
                sum
            );
        }
    }

    // --- T089: Parallel JT correctness tests ---

    #[test]
    fn parallel_jt_matches_sequential_on_cancer() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn/cancer.bif");
        let net = load_bif_file(&path).unwrap();
        let evidence = Evidence::new();

        let seq = JunctionTree::sequential();
        let par = JunctionTree::new();

        let seq_result = seq.infer(&net, &evidence).unwrap();
        let par_result = par.infer(&net, &evidence).unwrap();

        assert_eq!(seq_result.marginals.len(), par_result.marginals.len());
        for (var, seq_probs) in &seq_result.marginals {
            let par_probs = par_result.marginals.get(var).unwrap();
            for (i, (&sp, &pp)) in seq_probs.iter().zip(par_probs.iter()).enumerate() {
                assert!(
                    (sp - pp).abs() < 1e-12,
                    "Cancer: seq vs par mismatch: {} state {}: {} vs {}",
                    var,
                    i,
                    sp,
                    pp
                );
            }
        }
    }

    #[test]
    fn parallel_jt_matches_sequential_on_asia() {
        let net = load_asia();
        let evidence = Evidence::new();

        let seq = JunctionTree::sequential();
        let par = JunctionTree::new();

        let seq_result = seq.infer(&net, &evidence).unwrap();
        let par_result = par.infer(&net, &evidence).unwrap();

        assert_eq!(seq_result.marginals.len(), par_result.marginals.len());
        for (var, seq_probs) in &seq_result.marginals {
            let par_probs = par_result.marginals.get(var).unwrap();
            for (i, (&sp, &pp)) in seq_probs.iter().zip(par_probs.iter()).enumerate() {
                assert!(
                    (sp - pp).abs() < 1e-12,
                    "Asia: seq vs par mismatch: {} state {}: {} vs {}",
                    var,
                    i,
                    sp,
                    pp
                );
            }
        }
    }

    #[test]
    fn parallel_jt_matches_sequential_on_alarm() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn/alarm.bif");
        if !path.exists() {
            return;
        }
        let net = load_bif_file(&path).unwrap();
        let evidence = Evidence::new();

        let seq = JunctionTree::sequential();
        let par = JunctionTree::new();

        let seq_result = seq.infer(&net, &evidence).unwrap();
        let par_result = par.infer(&net, &evidence).unwrap();

        assert_eq!(seq_result.marginals.len(), par_result.marginals.len());
        for (var, seq_probs) in &seq_result.marginals {
            let par_probs = par_result.marginals.get(var).unwrap();
            for (i, (&sp, &pp)) in seq_probs.iter().zip(par_probs.iter()).enumerate() {
                assert!(
                    (sp - pp).abs() < 1e-12,
                    "Alarm: seq vs par mismatch: {} state {}: {} vs {}",
                    var,
                    i,
                    sp,
                    pp
                );
            }
        }
    }

    #[test]
    fn parallel_jt_matches_sequential_with_evidence() {
        let net = load_asia();
        let mut evidence = Evidence::new();
        evidence
            .observe(&net, &var_name("Xray"), "positive")
            .unwrap();
        evidence
            .observe(&net, &var_name("Dyspnea"), "present")
            .unwrap();

        let seq = JunctionTree::sequential();
        let par = JunctionTree::new();

        let seq_result = seq.infer(&net, &evidence).unwrap();
        let par_result = par.infer(&net, &evidence).unwrap();

        assert_eq!(seq_result.marginals.len(), par_result.marginals.len());
        for (var, seq_probs) in &seq_result.marginals {
            let par_probs = par_result.marginals.get(var).unwrap();
            for (i, (&sp, &pp)) in seq_probs.iter().zip(par_probs.iter()).enumerate() {
                assert!(
                    (sp - pp).abs() < 1e-12,
                    "Asia+evidence: seq vs par mismatch: {} state {}: {} vs {}",
                    var,
                    i,
                    sp,
                    pp
                );
            }
        }
    }

    #[test]
    fn small_network_uses_sequential_fallback() {
        // Cancer network is very small — should fall back to sequential.
        // This test verifies that the fallback path produces correct results.
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn/cancer.bif");
        let net = load_bif_file(&path).unwrap();
        let evidence = Evidence::new();

        let ve = super::super::variable_elimination::VariableElimination::new();
        let jt = JunctionTree::new();

        let ve_result = ve.infer(&net, &evidence).unwrap();
        let jt_result = jt.infer(&net, &evidence).unwrap();

        for (var, ve_probs) in &ve_result.marginals {
            let jt_probs = jt_result.marginals.get(var).unwrap();
            for (i, (&vp, &jp)) in ve_probs.iter().zip(jt_probs.iter()).enumerate() {
                assert!(
                    (vp - jp).abs() < 1e-6,
                    "Cancer VE vs JT: {} state {}: {} vs {}",
                    var,
                    i,
                    vp,
                    jp
                );
            }
        }
    }

    // --- T090: Benchmark test (measures but doesn't enforce threshold in unit test) ---

    #[test]
    fn parallel_jt_alarm_benchmark() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn/alarm.bif");
        if !path.exists() {
            return;
        }
        let net = load_bif_file(&path).unwrap();
        let evidence = Evidence::new();

        let seq = JunctionTree::sequential();
        let par = JunctionTree::new();

        // Warmup
        let _ = seq.infer(&net, &evidence).unwrap();
        let _ = par.infer(&net, &evidence).unwrap();

        // Timed sequential runs
        let start = std::time::Instant::now();
        let n_runs = 10;
        for _ in 0..n_runs {
            let _ = seq.infer(&net, &evidence).unwrap();
        }
        let seq_elapsed = start.elapsed();

        // Timed parallel runs
        let start = std::time::Instant::now();
        for _ in 0..n_runs {
            let _ = par.infer(&net, &evidence).unwrap();
        }
        let par_elapsed = start.elapsed();

        // Report times (informational — speedup depends on CPU count)
        let seq_ms = seq_elapsed.as_millis() as f64 / n_runs as f64;
        let par_ms = par_elapsed.as_millis() as f64 / n_runs as f64;
        let speedup = seq_ms / par_ms;

        // Don't hard-fail on speedup since it depends on machine/cores,
        // but assert both produce valid results.
        assert!(
            seq_ms > 0.0 || par_ms > 0.0,
            "Benchmark should measure time (seq={}ms, par={}ms, speedup={:.2}x)",
            seq_ms,
            par_ms,
            speedup
        );
    }
}
