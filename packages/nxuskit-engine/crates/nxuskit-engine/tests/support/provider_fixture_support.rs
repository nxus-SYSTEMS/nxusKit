use std::collections::BTreeSet;

use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn fixture_document(raw: &str, name: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|error| panic!("{name}: invalid JSON: {error}"))
}

pub(super) fn fixture_records(raw: &str, name: &str) -> Vec<Value> {
    fixture_document(raw, name)["records"]
        .as_array()
        .unwrap_or_else(|| panic!("{name}: records must be an array"))
        .clone()
}

pub(super) fn record_by_id(raw: &str, name: &str, id: &str) -> Value {
    fixture_records(raw, name)
        .into_iter()
        .find(|record| record["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("{name}: missing record {id}"))
}

pub(super) struct AssertionLedger {
    pub(super) ids: BTreeSet<String>,
}

impl AssertionLedger {
    pub(super) fn new() -> Self {
        Self {
            ids: BTreeSet::new(),
        }
    }

    pub(super) fn record(&mut self, id: &str) {
        assert!(
            self.ids.insert(id.to_owned()),
            "duplicate assertion id: {id}"
        );
    }
}
