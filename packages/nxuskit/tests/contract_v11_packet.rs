//! T021 (US1) — Regulated workflow packet round-trip + redaction-preservation
//! tests.
//!
//! Pins the public-safe generic regulated-workflow packet shape and the
//! redaction-flag round-trip rule from SC-011 / Edge Case "Round-trip with
//! redaction flags set".

use nxuskit::contract::v1_1::{
    CONTRACT_VERSION, DispositionKind, HumanReviewDisposition, RedactionFlag, RedactionKind,
    RegulatedWorkflowPacket, ReviewStatus,
};

fn sample_packet() -> RegulatedWorkflowPacket {
    RegulatedWorkflowPacket::builder("pkt_quickstart")
        .source_document("doc_42")
        .add_source_reference("file://input.txt#line=1-200")
        .push_evidence(
            "cand_01",
            "Generic candidate evidence extracted from source.",
            "file://input.txt#line=42",
        )
        .draft(
            "draft_01",
            "Generic draft output text, expert review required.",
        )
        .review_status(ReviewStatus::AwaitingReview)
        .add_redaction(RedactionFlag::new(
            "source_packet.source_references[0]",
            RedactionKind::Pii,
        ))
        .disposition(HumanReviewDisposition::pending())
        .build()
}

#[test]
fn packet_carries_contract_version_and_non_automated_decision_marker() {
    let packet = sample_packet();
    assert_eq!(packet.contract_version, CONTRACT_VERSION);
    assert!(packet.non_automated_decision_marker);
}

#[test]
fn packet_round_trip_preserves_redaction_flags_byte_equal() {
    let packet = sample_packet();
    let exported = packet.export_canonical_json().expect("export");
    let reimported = RegulatedWorkflowPacket::import_canonical_json(&exported).expect("import");
    // SC-011: redaction flags must round-trip byte-equal.
    assert_eq!(packet.redaction_flags, reimported.redaction_flags);
    // The exported canonical form must still contain the redaction.
    assert!(exported.contains("\"redaction_flags\""));
    assert!(exported.contains("\"pii\""));
}

#[test]
fn packet_with_no_redactions_round_trips_with_empty_vec() {
    let packet = RegulatedWorkflowPacket::builder("pkt_no_redactions")
        .source_document("doc_x")
        .draft("draft_x", "Generic draft.")
        .review_status(ReviewStatus::AwaitingReview)
        .disposition(HumanReviewDisposition::pending())
        .build();
    assert!(packet.redaction_flags.is_empty());
    let exported = packet.export_canonical_json().expect("export");
    let reimported = RegulatedWorkflowPacket::import_canonical_json(&exported).expect("import");
    assert!(reimported.redaction_flags.is_empty());
}

#[test]
fn review_status_enumerates_five_states_with_snake_case() {
    let all = [
        (ReviewStatus::AwaitingReview, "\"awaiting_review\""),
        (ReviewStatus::InReview, "\"in_review\""),
        (ReviewStatus::AcceptedByReviewer, "\"accepted_by_reviewer\""),
        (ReviewStatus::RejectedByReviewer, "\"rejected_by_reviewer\""),
        (ReviewStatus::NeedsRevision, "\"needs_revision\""),
    ];
    for (s, want) in all {
        assert_eq!(&serde_json::to_string(&s).unwrap(), want);
    }
}

#[test]
fn disposition_kind_enumerates_four_states_with_snake_case() {
    let all = [
        (DispositionKind::Pending, "\"pending\""),
        (DispositionKind::Accept, "\"accept\""),
        (DispositionKind::Reject, "\"reject\""),
        (DispositionKind::NeedsRevision, "\"needs_revision\""),
    ];
    for (d, want) in all {
        assert_eq!(&serde_json::to_string(&d).unwrap(), want);
    }
}

#[test]
fn redaction_kind_enumerates_five_states_with_snake_case() {
    let all = [
        (RedactionKind::Pii, "\"pii\""),
        (RedactionKind::Phi, "\"phi\""),
        (RedactionKind::Confidential, "\"confidential\""),
        (RedactionKind::ClientPrivilege, "\"client_privilege\""),
        (RedactionKind::Other, "\"other\""),
    ];
    for (r, want) in all {
        assert_eq!(&serde_json::to_string(&r).unwrap(), want);
    }
}
