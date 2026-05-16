//! BIF (Bayesian Interchange Format) file parser.
//!
//! Parses `.bif` files into `BayesianNetwork` structures using nom combinators.
//! Supports the standard BIF format used by bnlearn, pyAgrum, and pgmpy.

use nom::{
    IResult, Parser,
    bytes::complete::{tag, take_while1},
    character::complete::{char, multispace0, multispace1},
    multi::separated_list1,
    number::complete::double,
};

use super::error::{BayesError, BayesResult};
use super::network::BayesianNetwork;
use super::types::{DiscreteVariable, StateName, VariableName};

/// Parse a BIF-format string into a BayesianNetwork.
///
/// # Errors
/// Returns `BayesError::ParseError` if the input is malformed.
pub fn parse_bif(input: &str) -> BayesResult<BayesianNetwork> {
    let mut network = BayesianNetwork::new();

    // Strip comments first (// to end of line)
    let cleaned = strip_comments(input);
    let input = cleaned.as_str();

    // Parse network declaration
    let input = parse_network_decl(input)
        .map(|(rest, _)| rest)
        .map_err(|e| {
            BayesError::ParseError(format!("Failed to parse network declaration: {}", e))
        })?;

    // Parse all blocks (variable or probability)
    let mut remaining = input;
    while !remaining.trim().is_empty() {
        // Skip whitespace
        let (rest, _) = multispace0::<&str, nom::error::Error<&str>>
            .parse(remaining)
            .map_err(|e| BayesError::ParseError(format!("Whitespace error: {}", e)))?;
        if rest.is_empty() {
            break;
        }

        if rest.starts_with("variable") {
            let (rest, (name, states)) = parse_variable_block(rest).map_err(|e| {
                BayesError::ParseError(format!("Failed to parse variable block: {}", e))
            })?;
            let var = DiscreteVariable::new(
                VariableName::new(name)?,
                states
                    .into_iter()
                    .map(StateName::new)
                    .collect::<Result<Vec<_>, _>>()?,
            )?;
            network.add_variable(var)?;
            remaining = rest;
        } else if rest.starts_with("probability") {
            let (rest, (child, parents, probs)) = parse_probability_block(rest).map_err(|e| {
                BayesError::ParseError(format!("Failed to parse probability block: {}", e))
            })?;
            let child_name = VariableName::new(child)?;

            // Add edges from parents to child
            for parent in &parents {
                let parent_name = VariableName::new(parent.as_str())?;
                if !network.parents(&child_name).contains(&parent_name) {
                    network.add_edge(&parent_name, &child_name)?;
                }
            }

            network.set_cpt(&child_name, probs)?;
            remaining = rest;
        } else {
            break;
        }
    }

    Ok(network)
}

/// Strip // comments from input.
fn strip_comments(input: &str) -> String {
    input
        .lines()
        .map(|line| {
            if let Some(pos) = line.find("//") {
                &line[..pos]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn identifier(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_alphanumeric() || c == '_').parse(input)
}

fn state_name_chars(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '-').parse(input)
}

/// Comma separator with optional whitespace.
fn comma_sep(input: &str) -> IResult<&str, char> {
    let (input, _) = multispace0.parse(input)?;
    let (input, c) = char(',').parse(input)?;
    let (input, _) = multispace0.parse(input)?;
    Ok((input, c))
}

/// Parse: network <name> { }
fn parse_network_decl(input: &str) -> IResult<&str, ()> {
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = tag("network").parse(input)?;
    let (input, _) = multispace1.parse(input)?;
    let (input, _) = identifier(input)?;
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = char('{').parse(input)?;
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = char('}').parse(input)?;
    Ok((input, ()))
}

/// Parse a variable block: variable <name> { type discrete [ N ] { s1, s2, ... }; }
fn parse_variable_block(input: &str) -> IResult<&str, (&str, Vec<&str>)> {
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = tag("variable").parse(input)?;
    let (input, _) = multispace1.parse(input)?;
    let (input, name) = identifier(input)?;
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = char('{').parse(input)?;
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = tag("type").parse(input)?;
    let (input, _) = multispace1.parse(input)?;
    let (input, _) = tag("discrete").parse(input)?;
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = char('[').parse(input)?;
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = take_while1(|c: char| c.is_ascii_digit()).parse(input)?;
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = char(']').parse(input)?;
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = char('{').parse(input)?;
    let (input, _) = multispace0.parse(input)?;

    let (input, states) = separated_list1(comma_sep, state_name_chars).parse(input)?;

    let (input, _) = multispace0.parse(input)?;
    let (input, _) = char('}').parse(input)?;
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = char(';').parse(input)?;
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = char('}').parse(input)?;

    Ok((input, (name, states)))
}

/// Parse a probability block: probability ( child | parent1, parent2, ... ) { ... }
fn parse_probability_block(input: &str) -> IResult<&str, (&str, Vec<String>, Vec<f64>)> {
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = tag("probability").parse(input)?;
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = char('(').parse(input)?;
    let (input, _) = multispace0.parse(input)?;

    let (input, child) = identifier(input)?;
    let (input, _) = multispace0.parse(input)?;

    // Parse optional parent list
    let (input, parents) = if input.starts_with('|') {
        let (input, _) = char('|').parse(input)?;
        let (input, _) = multispace0.parse(input)?;
        let (input, parents) = separated_list1(comma_sep, identifier).parse(input)?;
        (input, parents.into_iter().map(String::from).collect())
    } else {
        (input, vec![])
    };

    let (input, _) = multispace0.parse(input)?;
    let (input, _) = char(')').parse(input)?;
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = char('{').parse(input)?;
    let (input, _) = multispace0.parse(input)?;

    // Parse probability entries
    let (input, probs) = if parents.is_empty() {
        parse_table_entry(input)?
    } else {
        parse_conditional_entries(input)?
    };

    let (input, _) = multispace0.parse(input)?;
    let (input, _) = char('}').parse(input)?;

    Ok((input, (child, parents, probs)))
}

/// Parse: table v1, v2, ...;
fn parse_table_entry(input: &str) -> IResult<&str, Vec<f64>> {
    let (input, _) = tag("table").parse(input)?;
    let (input, _) = multispace0.parse(input)?;
    let (input, probs) = separated_list1(comma_sep, double).parse(input)?;
    let (input, _) = multispace0.parse(input)?;
    let (input, _) = char(';').parse(input)?;
    Ok((input, probs))
}

/// Parse conditional entries: (s1, s2) v1, v2; (s3, s4) v3, v4; ...
fn parse_conditional_entries(input: &str) -> IResult<&str, Vec<f64>> {
    let mut all_probs = Vec::new();
    let mut remaining = input;

    loop {
        let (input, _) = multispace0.parse(remaining)?;
        if !input.starts_with('(') {
            remaining = input;
            break;
        }
        let (input, _) = char('(').parse(input)?;
        let (input, _) = multispace0.parse(input)?;

        // Skip parent state names
        let (input, _states) = separated_list1(comma_sep, state_name_chars).parse(input)?;

        let (input, _) = multispace0.parse(input)?;
        let (input, _) = char(')').parse(input)?;
        let (input, _) = multispace0.parse(input)?;

        let (input, probs) = separated_list1(comma_sep, double).parse(input)?;
        let (input, _) = multispace0.parse(input)?;
        let (input, _) = char(';').parse(input)?;

        all_probs.extend(probs);
        remaining = input;
    }

    Ok((remaining, all_probs))
}

/// Load a BIF file from a path.
pub fn load_bif_file(path: &std::path::Path) -> BayesResult<BayesianNetwork> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        BayesError::ParseError(format!("Failed to read file '{}': {}", path.display(), e))
    })?;
    parse_bif(&content)
}

/// Save a BayesianNetwork to a BIF file.
///
/// Serializes discrete variables, edges, and CPTs in standard BIF format
/// with 17-digit precision for probability values (ensuring round-trip fidelity).
///
/// # Errors
/// - `BayesError::IncompleteNetwork` if any discrete variable lacks a CPT.
pub fn save_bif_file(network: &BayesianNetwork, path: &std::path::Path) -> BayesResult<()> {
    let content = serialize_bif(network)?;
    std::fs::write(path, content).map_err(|e| {
        BayesError::ParseError(format!("Failed to write file '{}': {}", path.display(), e))
    })?;
    Ok(())
}

/// Serialize a BayesianNetwork to a BIF-format string.
pub fn serialize_bif(network: &BayesianNetwork) -> BayesResult<String> {
    // Validate: all discrete variables must have CPTs.
    network.validate()?;

    let mut out = String::new();

    // Network declaration.
    out.push_str("network unknown {\n}\n\n");

    // Use topological order for variable and probability blocks.
    let topo = network.topological_sort();

    // Variable blocks.
    for vname in &topo {
        if let Some(var) = network.variable(vname) {
            out.push_str(&format!(
                "variable {} {{\n",
                escape_bif_name(vname.as_str())
            ));
            out.push_str(&format!(
                "  type discrete [ {} ] {{ {} }};\n",
                var.cardinality(),
                var.states
                    .iter()
                    .map(|s| escape_bif_name(s.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            out.push_str("}\n\n");
        }
    }

    // Gaussian variable blocks (if any).
    for vname in network.gaussian_variables().keys() {
        if let Some(gv) = network.gaussian_variable(vname) {
            out.push_str(&format!(
                "// gaussian variable {} : mean_base={:.17e}, variance={:.17e}\n",
                escape_bif_name(vname.as_str()),
                gv.mean_base,
                gv.variance
            ));
            for (pname, w) in &gv.weights {
                out.push_str(&format!(
                    "//   weight {} = {:.17e}\n",
                    escape_bif_name(pname.as_str()),
                    w
                ));
            }
            out.push('\n');
        }
    }

    // Probability blocks.
    for vname in &topo {
        if let Some(factor) = network.cpt(vname) {
            let var = network.variable(vname).unwrap();
            let parents = network.parents(vname);

            if parents.is_empty() {
                // Root node: table format.
                out.push_str(&format!(
                    "probability ( {} ) {{\n",
                    escape_bif_name(vname.as_str())
                ));
                let probs: Vec<f64> = factor.log_values.iter().map(|lv| lv.exp()).collect();
                out.push_str(&format!(
                    "  table {};\n",
                    probs
                        .iter()
                        .map(|p| format!("{:.17e}", p))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                out.push_str("}\n\n");
            } else {
                // Conditional: (parent_states) probs format.
                out.push_str(&format!(
                    "probability ( {} | {} ) {{\n",
                    escape_bif_name(vname.as_str()),
                    parents
                        .iter()
                        .map(|p| escape_bif_name(p.as_str()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));

                let child_card = var.cardinality();
                let num_parent_configs = factor.size() / child_card;

                for config_idx in 0..num_parent_configs {
                    // Decode parent configuration.
                    let assignment = factor.index_to_assignment(config_idx * child_card);
                    let parent_states: Vec<String> = parents
                        .iter()
                        .enumerate()
                        .map(|(pi, pname)| {
                            let parent_var = network.variable(pname).unwrap();
                            let state_idx = assignment[pi].value();
                            escape_bif_name(parent_var.states[state_idx].as_str())
                        })
                        .collect();

                    let row_start = config_idx * child_card;
                    let row_end = row_start + child_card;
                    let row_probs: Vec<f64> = factor.log_values[row_start..row_end]
                        .iter()
                        .map(|lv| lv.exp())
                        .collect();

                    out.push_str(&format!(
                        "  ({}) {};\n",
                        parent_states.join(", "),
                        row_probs
                            .iter()
                            .map(|p| format!("{:.17e}", p))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }

                out.push_str("}\n\n");
            }
        }
    }

    Ok(out)
}

/// Escape a BIF name if it contains special characters.
fn escape_bif_name(name: &str) -> String {
    if name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        name.to_string()
    } else {
        format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_network() {
        let bif = r#"
network test {
}

variable A {
  type discrete [ 2 ] { yes, no };
}

probability ( A ) {
  table 0.3, 0.7;
}
"#;
        let net = parse_bif(bif).unwrap();
        assert_eq!(net.num_variables(), 1);
        assert!(net.is_complete());
    }

    #[test]
    fn parse_two_variable_network() {
        let bif = r#"
network test {
}

variable A {
  type discrete [ 2 ] { yes, no };
}

variable B {
  type discrete [ 2 ] { true, false };
}

probability ( A ) {
  table 0.6, 0.4;
}

probability ( B | A ) {
  (yes) 0.2, 0.8;
  (no)  0.75, 0.25;
}
"#;
        let net = parse_bif(bif).unwrap();
        assert_eq!(net.num_variables(), 2);
        assert_eq!(net.num_edges(), 1);
        assert!(net.is_complete());
    }

    #[test]
    fn parse_with_comments() {
        let bif = r#"
// This is a comment
network test {
}
// Another comment
variable A {
  type discrete [ 2 ] { yes, no };
}

probability ( A ) {
  table 0.5, 0.5;
}
"#;
        let net = parse_bif(bif).unwrap();
        assert_eq!(net.num_variables(), 1);
    }

    #[test]
    fn parse_asia_fixture() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn/asia.bif");
        if !path.exists() {
            return;
        }
        let net = load_bif_file(&path).unwrap();
        assert_eq!(net.num_variables(), 8);
        assert_eq!(net.num_edges(), 8);
        assert!(net.is_complete());
    }

    #[test]
    fn parse_cancer_fixture() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn/cancer.bif");
        if !path.exists() {
            return;
        }
        let net = load_bif_file(&path).unwrap();
        assert_eq!(net.num_variables(), 5);
        assert_eq!(net.num_edges(), 4);
        assert!(net.is_complete());
    }

    #[test]
    fn parse_earthquake_fixture() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/bn/earthquake.bif");
        if !path.exists() {
            return;
        }
        let net = load_bif_file(&path).unwrap();
        assert_eq!(net.num_variables(), 5);
        assert_eq!(net.num_edges(), 4);
        assert!(net.is_complete());
    }

    #[test]
    fn parse_alarm_fixture() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn/alarm.bif");
        if !path.exists() {
            return;
        }
        let net = load_bif_file(&path).unwrap();
        assert_eq!(net.num_variables(), 37);
        assert_eq!(net.num_edges(), 46);
        assert!(net.is_complete());
    }

    #[test]
    fn parse_malformed_missing_closing_brace() {
        let bif = r#"
network test {
}

variable A {
  type discrete [ 2 ] { yes, no };
"#;
        assert!(parse_bif(bif).is_err());
    }

    // === BIF Export Tests ===

    fn fixture_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn")
    }

    /// Round-trip: load → save → reload → verify identical structure and CPTs.
    fn roundtrip_verify(name: &str) {
        let path = fixture_dir().join(name);
        if !path.exists() {
            return;
        }
        let original = load_bif_file(&path).unwrap();

        // Serialize to string, then parse back.
        let bif_str = serialize_bif(&original).unwrap();
        let reloaded = parse_bif(&bif_str).unwrap();

        // Same number of variables.
        assert_eq!(
            original.num_variables(),
            reloaded.num_variables(),
            "Variable count mismatch for {}",
            name
        );

        // Same number of edges.
        assert_eq!(
            original.num_edges(),
            reloaded.num_edges(),
            "Edge count mismatch for {}",
            name
        );

        // Same CPT values for each variable.
        for vname in original.variable_names() {
            if let Some(orig_cpt) = original.cpt(&vname) {
                let reload_cpt = reloaded.cpt(&vname).unwrap_or_else(|| {
                    panic!("Missing CPT for variable '{}' in reloaded network", vname)
                });

                // Same dimensions.
                assert_eq!(
                    orig_cpt.log_values.len(),
                    reload_cpt.log_values.len(),
                    "CPT size mismatch for '{}'",
                    vname
                );

                // Values match within 15 significant digits.
                for (i, (orig_lv, reload_lv)) in orig_cpt
                    .log_values
                    .iter()
                    .zip(reload_cpt.log_values.iter())
                    .enumerate()
                {
                    let orig_p = orig_lv.exp();
                    let reload_p = reload_lv.exp();
                    let diff = (orig_p - reload_p).abs();
                    let tol = 1e-14 * orig_p.abs().max(1e-300);
                    assert!(
                        diff < tol,
                        "CPT value mismatch for '{}' entry {}: {} vs {} (diff {})",
                        vname,
                        i,
                        orig_p,
                        reload_p,
                        diff
                    );
                }
            }
        }
    }

    #[test]
    fn roundtrip_cancer() {
        roundtrip_verify("cancer.bif");
    }

    #[test]
    fn roundtrip_asia() {
        roundtrip_verify("asia.bif");
    }

    #[test]
    fn roundtrip_alarm() {
        roundtrip_verify("alarm.bif");
    }

    #[test]
    fn save_bif_incomplete_network_fails() {
        let mut net = BayesianNetwork::new();
        net.add_variable(
            DiscreteVariable::new(
                VariableName::new("A").unwrap(),
                vec![
                    StateName::new("yes").unwrap(),
                    StateName::new("no").unwrap(),
                ],
            )
            .unwrap(),
        )
        .unwrap();
        // No CPT set → validate() should fail.
        let result = serialize_bif(&net);
        assert!(result.is_err());
    }

    #[test]
    fn save_bif_empty_network_succeeds() {
        let net = BayesianNetwork::new();
        let bif = serialize_bif(&net).unwrap();
        assert!(bif.contains("network unknown"));
    }

    #[test]
    fn save_bif_file_roundtrip() {
        let path = fixture_dir().join("cancer.bif");
        if !path.exists() {
            return;
        }
        let original = load_bif_file(&path).unwrap();

        let tmpdir = std::env::temp_dir();
        let tmpfile = tmpdir.join("test_cancer_roundtrip.bif");
        save_bif_file(&original, &tmpfile).unwrap();

        let reloaded = load_bif_file(&tmpfile).unwrap();
        assert_eq!(original.num_variables(), reloaded.num_variables());
        assert_eq!(original.num_edges(), reloaded.num_edges());

        // Clean up.
        let _ = std::fs::remove_file(&tmpfile);
    }
}
