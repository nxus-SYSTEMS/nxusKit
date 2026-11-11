//! Helpers for CE boundary and seeded-leak tests.
//!
//! tier: ce-boundary test utility

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeakClass {
    InternalSymbol,
    ProPath,
    ProDoc,
    ProDependency,
    NonOssEditionAccepted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryLeak {
    pub class: LeakClass,
    pub path: PathBuf,
    pub matched: String,
}

pub fn scan_ce_boundary(root: &Path) -> io::Result<Vec<BoundaryLeak>> {
    let mut leaks = Vec::new();
    scan_path(root, &mut leaks)?;
    Ok(leaks)
}

fn scan_path(path: &Path, leaks: &mut Vec<BoundaryLeak>) -> io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            scan_path(&entry?.path(), leaks)?;
        }
        return Ok(());
    }

    if !path.is_file() {
        return Ok(());
    }

    let Ok(contents) = fs::read_to_string(path) else {
        return Ok(());
    };

    for (class, needle) in leak_needles() {
        if contents.contains(needle) {
            leaks.push(BoundaryLeak {
                class,
                path: path.to_path_buf(),
                matched: needle.to_string(),
            });
        }
    }

    Ok(())
}

fn leak_needles() -> Vec<(LeakClass, &'static str)> {
    vec![
        (LeakClass::InternalSymbol, "ProviderChannel"),
        (LeakClass::InternalSymbol, "ToolCatalog"),
        (LeakClass::InternalSymbol, "CapabilityTruthSnapshot"),
        (LeakClass::InternalSymbol, "RequestEnvelope"),
        (LeakClass::InternalSymbol, "ResultEnvelope"),
        (LeakClass::InternalSymbol, "InternalChannel"),
        (
            LeakClass::ProPath,
            "packages/nxuskit-engine/crates/nxuskit-engine/src/pro",
        ),
        (LeakClass::ProPath, "nxuskit_engine::pro"),
        (LeakClass::ProDoc, "Pro internal routing"),
        (LeakClass::ProDependency, "nxuskit-pro"),
        (LeakClass::NonOssEditionAccepted, "NXUSKIT_EDITION=pro"),
        (LeakClass::NonOssEditionAccepted, "NXUSKIT_EDITION=internal"),
    ]
}
