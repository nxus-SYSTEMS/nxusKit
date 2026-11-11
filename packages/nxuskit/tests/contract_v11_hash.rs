//! T009 — Canonical hash determinism tests.
//!
//! Pins the SHA-256-over-sorted-canonical-JSON contract (research.md §4–5)
//! and the deterministic 12-char display-short projection. A stub that
//! returned a constant hash, a non-deterministic hash, or a non-prefix
//! display-short would fail every test here.

use nxuskit::contract::v1_1::{canonical_hash, canonical_json_bytes, display_short};
use serde_json::json;

#[test]
fn canonical_hash_is_sha256_hex_64_lowercase() {
    let hash = canonical_hash(&json!({"a": 1})).unwrap();
    let s = hash.as_str();
    assert_eq!(s.len(), 64, "canonical hash must be 64 hex chars");
    assert!(
        s.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
        "canonical hash must be lowercase hex: {s}"
    );
}

#[test]
fn canonical_hash_of_empty_object_matches_known_value() {
    // sha256 of `{}` (the canonical encoding of an empty JSON object) is
    // pinned so any drift in the encoder is detected. The exact value below
    // is sha256("{}").
    let hash = canonical_hash(&json!({})).unwrap();
    assert_eq!(
        hash.as_str(),
        "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
    );
}

#[test]
fn canonical_hash_is_invariant_under_key_reordering() {
    let a = json!({"alpha": 1, "beta": 2, "gamma": 3});
    let b = json!({"gamma": 3, "alpha": 1, "beta": 2});
    let c = json!({"beta": 2, "gamma": 3, "alpha": 1});

    let ha = canonical_hash(&a).unwrap();
    let hb = canonical_hash(&b).unwrap();
    let hc = canonical_hash(&c).unwrap();
    assert_eq!(ha, hb, "key reordering must not change hash");
    assert_eq!(hb, hc, "key reordering must not change hash");
}

#[test]
fn canonical_hash_is_invariant_under_whitespace_in_input_strings_only_when_field_value_unchanged() {
    // The JSON Value preserves string content verbatim; the canonicalizer
    // controls whitespace between tokens, not whitespace inside string
    // values. Identical Value trees must yield identical hashes.
    let a = json!({"k": "v"});
    let b = serde_json::from_str::<serde_json::Value>(r#"{  "k"  :  "v"  }"#).unwrap();
    assert_eq!(canonical_hash(&a).unwrap(), canonical_hash(&b).unwrap());
}

#[test]
fn canonical_hash_changes_when_content_changes() {
    let a = json!({"k": "v1"});
    let b = json!({"k": "v2"});
    assert_ne!(canonical_hash(&a).unwrap(), canonical_hash(&b).unwrap());
}

#[test]
fn canonical_hash_recurses_into_nested_objects() {
    let a = json!({"outer": {"alpha": 1, "beta": 2}});
    let b = json!({"outer": {"beta": 2, "alpha": 1}});
    assert_eq!(canonical_hash(&a).unwrap(), canonical_hash(&b).unwrap());

    let c = json!({"outer": {"alpha": 1, "beta": 3}});
    assert_ne!(canonical_hash(&a).unwrap(), canonical_hash(&c).unwrap());
}

#[test]
fn canonical_hash_preserves_array_order() {
    // Arrays are sequences. Reordering MUST change the hash even when the
    // element set is identical.
    let a = json!([1, 2, 3]);
    let b = json!([3, 2, 1]);
    assert_ne!(canonical_hash(&a).unwrap(), canonical_hash(&b).unwrap());
}

#[test]
fn canonical_json_bytes_emit_no_whitespace_between_tokens() {
    let v = json!({"a": [1, 2, {"b": "c"}]});
    let bytes = canonical_json_bytes(&v).unwrap();
    let s = std::str::from_utf8(&bytes).unwrap();
    assert!(
        !s.contains(' ') && !s.contains('\n') && !s.contains('\t'),
        "canonical bytes must have no inter-token whitespace, got: {s}"
    );
}

#[test]
fn display_short_is_prefix_of_canonical_hash() {
    for value in [
        json!({}),
        json!({"k": "v"}),
        json!({"alpha": [1, 2, 3], "beta": "value"}),
    ] {
        let hash = canonical_hash(&value).unwrap();
        let short = display_short(&hash);
        assert_eq!(short.as_str().len(), 12);
        assert_eq!(
            short.as_str(),
            &hash.as_str()[..12],
            "display_short must be the first 12 hex chars of the canonical hash"
        );
    }
}

#[test]
fn display_short_is_deterministic_across_invocations() {
    let v = json!({"k": "v"});
    let h1 = canonical_hash(&v).unwrap();
    let h2 = canonical_hash(&v).unwrap();
    assert_eq!(h1, h2);
    assert_eq!(display_short(&h1), display_short(&h2));
}
