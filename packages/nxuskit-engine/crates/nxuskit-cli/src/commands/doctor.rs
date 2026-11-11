//! `nxuskit-cli doctor` — v1.1.0 public readiness projection.
//!
//! US1 intentionally emits a public-safe snapshot built from the canonical
//! `nxuskit::contract::v1_1` DTOs. Live FFI/provider probing is
//! an Integration follow-up through the narrow contract adapter; this command
//! must not expose internal routing, catalog, or repo topology.

use clap::Args;
use nxuskit::contract::v1_1::doctor_readiness_snapshot;

/// Arguments accepted by `nxuskit-cli doctor`.
#[derive(Args, Debug, Clone)]
pub struct DoctorArgs {
    /// Emit machine-readable JSON projection of `ReadinessSnapshot`.
    #[arg(long, default_value_t = false)]
    pub json: bool,

    /// Limit to specific provider IDs (repeatable).
    #[arg(long = "provider", value_name = "ID")]
    pub providers: Vec<String>,

    /// Per-provider probe timeout override.
    #[arg(long, default_value_t = 400u64)]
    pub probe_timeout_ms: u64,
}

/// Emit the public readiness projection.
pub fn run(args: &DoctorArgs) -> i32 {
    let snapshot = doctor_readiness_snapshot(&args.providers, args.probe_timeout_ms);
    if args.json {
        match serde_json::to_string(&snapshot) {
            Ok(body) => {
                println!("{body}");
                0
            }
            Err(e) => {
                eprintln!("nxuskit-cli doctor: failed to serialize readiness snapshot: {e}");
                1
            }
        }
    } else {
        println!(
            "nxuskit-cli doctor readiness: {} provider(s), {} capability entries",
            snapshot.providers.len(),
            snapshot.capabilities.len()
        );
        0
    }
}
