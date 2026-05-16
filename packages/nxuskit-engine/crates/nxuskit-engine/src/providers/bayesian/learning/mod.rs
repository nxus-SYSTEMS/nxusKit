//! Parameter learning for Bayesian Networks.
//!
//! Provides dataset loading (CSV) and parameter estimation algorithms:
//! - **MLE** (Maximum Likelihood Estimation) with Laplace smoothing
//! - **Bayesian** learning with Dirichlet priors (planned)

pub mod bayesian;
pub mod hill_climb;
pub mod k2;
pub mod mle;
pub mod scoring;

use std::collections::HashMap;
use std::path::Path;

use crate::providers::bayesian::error::{BayesError, BayesResult};
use crate::providers::bayesian::network::BayesianNetwork;
use crate::providers::bayesian::types::{StateIndex, VariableName};

/// A tabular dataset of discrete observations for parameter learning.
///
/// Each row maps variable names to observed state indices. Missing values
/// are represented as `None` (for available-case / complete-case strategies).
#[derive(Debug, Clone)]
pub struct Dataset {
    /// Column names (variable names in the network).
    pub columns: Vec<VariableName>,
    /// Rows of observations. Each row maps column index → `Option<StateIndex>`.
    /// `None` = missing value.
    pub rows: Vec<Vec<Option<StateIndex>>>,
}

impl Dataset {
    /// Load a dataset from a CSV file.
    ///
    /// Column headers are mapped to `VariableName`s. Cell values are mapped
    /// to `StateIndex` via the network's variable definitions.
    ///
    /// - Missing values: empty cells or "?" are treated as `None`.
    /// - Extra columns (not in network) are silently ignored.
    /// - Missing columns (in network but not in CSV) cause an error.
    pub fn from_csv(path: &Path, network: &BayesianNetwork) -> BayesResult<Self> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(true)
            .from_path(path)
            .map_err(|e| {
                BayesError::ParseError(format!("Failed to open CSV '{}': {}", path.display(), e))
            })?;

        let headers: Vec<String> = reader
            .headers()
            .map_err(|e| BayesError::ParseError(format!("Failed to read CSV headers: {}", e)))?
            .iter()
            .map(|h| h.trim().to_string())
            .collect();

        // Map header names to network variables, building column→variable index.
        let net_var_names = network.variable_names();
        let mut col_map: Vec<Option<(usize, VariableName)>> = Vec::with_capacity(headers.len());
        let mut found_vars: HashMap<String, usize> = HashMap::new();

        for (csv_col, header) in headers.iter().enumerate() {
            if let Ok(vn) = VariableName::new(header) {
                if network.variable(&vn).is_some() {
                    found_vars.insert(header.clone(), csv_col);
                    col_map.push(Some((csv_col, vn)));
                } else {
                    col_map.push(None); // extra column — ignored
                }
            } else {
                col_map.push(None); // invalid variable name — ignored
            }
        }

        // Check all network variables are present in CSV.
        for vn in &net_var_names {
            if !found_vars.contains_key(vn.as_str()) {
                return Err(BayesError::MissingColumn(format!(
                    "Network variable '{}' not found in CSV columns: {:?}",
                    vn, headers
                )));
            }
        }

        // Build ordered column list (only network variables, in network order).
        let columns: Vec<VariableName> = net_var_names;
        let col_indices: Vec<usize> = columns
            .iter()
            .map(|vn| *found_vars.get(vn.as_str()).unwrap())
            .collect();

        // Read rows.
        let mut rows = Vec::new();
        for result in reader.records() {
            let record = result
                .map_err(|e| BayesError::ParseError(format!("Failed to read CSV row: {}", e)))?;

            let mut row = Vec::with_capacity(columns.len());
            for (col_idx, vn) in col_indices.iter().zip(columns.iter()) {
                let cell = record.get(*col_idx).unwrap_or("").trim();
                if cell.is_empty() || cell == "?" {
                    row.push(None);
                } else {
                    let var = network.variable(vn).unwrap();
                    match var.state_index(cell) {
                        Some(si) => row.push(Some(si)),
                        None => row.push(None), // unknown state treated as missing
                    }
                }
            }
            rows.push(row);
        }

        if rows.is_empty() {
            return Err(BayesError::EmptyDataset(
                "CSV file contains no data rows".into(),
            ));
        }

        Ok(Dataset { columns, rows })
    }

    /// Create a dataset from in-memory data (for testing).
    pub fn from_rows(
        columns: Vec<VariableName>,
        rows: Vec<Vec<Option<StateIndex>>>,
    ) -> BayesResult<Self> {
        if rows.is_empty() {
            return Err(BayesError::EmptyDataset("No data rows provided".into()));
        }
        for (i, row) in rows.iter().enumerate() {
            if row.len() != columns.len() {
                return Err(BayesError::ParseError(format!(
                    "Row {} has {} values but {} columns expected",
                    i,
                    row.len(),
                    columns.len()
                )));
            }
        }
        Ok(Dataset { columns, rows })
    }

    /// Number of rows.
    pub fn num_rows(&self) -> usize {
        self.rows.len()
    }

    /// Number of columns.
    pub fn num_columns(&self) -> usize {
        self.columns.len()
    }

    /// Get column index for a variable name.
    pub fn column_index(&self, name: &VariableName) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }
}

/// Trait for parameter learning algorithms.
pub trait ParameterLearner {
    /// Learn CPT parameters from data and set them on the network.
    fn fit(&self, network: &mut BayesianNetwork, data: &Dataset) -> BayesResult<()>;
}

/// Result of structure learning.
#[derive(Debug, Clone)]
pub struct StructureSearchResult {
    /// The discovered network structure.
    pub network: BayesianNetwork,
    /// The score of the discovered structure.
    pub score: f64,
    /// Number of search iterations performed.
    pub iterations: usize,
}

/// Trait for structure learning algorithms.
pub trait StructureLearner {
    /// Discover network structure from data.
    ///
    /// The input network provides variable definitions (names, states) but its
    /// edges will be replaced by the discovered structure. CPTs are NOT set.
    fn search(
        &self,
        variables: &BayesianNetwork,
        data: &Dataset,
    ) -> BayesResult<StructureSearchResult>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::bayesian::bif::load_bif_file;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn fixture_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn")
    }

    fn load_cancer() -> BayesianNetwork {
        load_bif_file(&fixture_dir().join("cancer.bif")).unwrap()
    }

    fn write_csv(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn dataset_from_csv_basic() {
        let net = load_cancer();
        let csv = write_csv(
            "Pollution,Smoker,Cancer,Xray,Dyspnea\n\
             low,True,True,positive,True\n\
             high,False,False,negative,False\n",
        );
        let ds = Dataset::from_csv(csv.path(), &net).unwrap();
        assert_eq!(ds.num_rows(), 2);
        assert_eq!(ds.num_columns(), 5);
    }

    #[test]
    fn dataset_missing_values() {
        let net = load_cancer();
        let csv = write_csv(
            "Pollution,Smoker,Cancer,Xray,Dyspnea\n\
             low,True,?,positive,True\n\
             high,,False,negative,\n",
        );
        let ds = Dataset::from_csv(csv.path(), &net).unwrap();
        assert_eq!(ds.num_rows(), 2);

        // Row 0: Cancer is "?" → None
        let cancer_col = ds
            .column_index(&VariableName::new("Cancer").unwrap())
            .unwrap();
        assert!(ds.rows[0][cancer_col].is_none());

        // Row 1: Smoker is empty → None
        let smoker_col = ds
            .column_index(&VariableName::new("Smoker").unwrap())
            .unwrap();
        assert!(ds.rows[1][smoker_col].is_none());
    }

    #[test]
    fn dataset_extra_columns_ignored() {
        let net = load_cancer();
        let csv = write_csv(
            "Pollution,Smoker,Cancer,Xray,Dyspnea,ExtraCol\n\
             low,True,True,positive,True,whatever\n",
        );
        let ds = Dataset::from_csv(csv.path(), &net).unwrap();
        assert_eq!(ds.num_columns(), 5); // only network variables
        assert_eq!(ds.num_rows(), 1);
    }

    #[test]
    fn dataset_missing_column_error() {
        let net = load_cancer();
        let csv = write_csv(
            "Pollution,Smoker,Cancer,Xray\n\
             low,True,True,positive\n",
        );
        let result = Dataset::from_csv(csv.path(), &net);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            format!("{}", err).contains("Dyspnea"),
            "Error should mention missing column: {}",
            err
        );
    }

    #[test]
    fn dataset_empty_csv_error() {
        let net = load_cancer();
        let csv = write_csv("Pollution,Smoker,Cancer,Xray,Dyspnea\n");
        let result = Dataset::from_csv(csv.path(), &net);
        assert!(result.is_err());
    }

    #[test]
    fn dataset_from_rows() {
        let cols = vec![
            VariableName::new("A").unwrap(),
            VariableName::new("B").unwrap(),
        ];
        let rows = vec![
            vec![Some(StateIndex::new(0)), Some(StateIndex::new(1))],
            vec![Some(StateIndex::new(1)), None],
        ];
        let ds = Dataset::from_rows(cols, rows).unwrap();
        assert_eq!(ds.num_rows(), 2);
        assert_eq!(ds.num_columns(), 2);
    }

    #[test]
    fn dataset_row_count() {
        let net = load_cancer();
        let csv = write_csv(
            "Pollution,Smoker,Cancer,Xray,Dyspnea\n\
             low,True,True,positive,True\n\
             high,False,False,negative,False\n\
             low,True,False,positive,True\n",
        );
        let ds = Dataset::from_csv(csv.path(), &net).unwrap();
        assert_eq!(ds.num_rows(), 3);
    }
}
