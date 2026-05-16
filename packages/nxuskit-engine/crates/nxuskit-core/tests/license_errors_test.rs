use nxuskit_core::license::LicenseError;
use nxuskit_core::license_types::TokenType;

#[test]
fn license_errors_expose_stable_machine_readable_codes() {
    let cases = [
        (
            LicenseError::Expired {
                token_type: TokenType::Developer,
            },
            "expired_subscription",
        ),
        (LicenseError::CancelledPurchase, "cancelled_purchase"),
        (
            LicenseError::MachineMismatch {
                expected: "sha256:expected".to_string(),
                actual: "sha256:actual".to_string(),
            },
            "machine_id_mismatch",
        ),
        (
            LicenseError::EnvironmentMismatch {
                expected: "production".to_string(),
                actual: "development".to_string(),
            },
            "environment_mismatch",
        ),
        (
            LicenseError::DeprecatedSigningKey {
                kid: "es256-old".to_string(),
            },
            "deprecated_signing_key",
        ),
        (
            LicenseError::MalformedToken {
                details: "not a JWT".to_string(),
            },
            "malformed_token",
        ),
        (
            LicenseError::InvalidProductId {
                expected: "nxuskit".to_string(),
                actual: "other-product".to_string(),
            },
            "wrong_product_identifier",
        ),
        (
            LicenseError::AuthenticationRequired,
            "authentication_required",
        ),
    ];

    for (error, code) in cases {
        assert_eq!(error.code(), code);
        assert_ne!(error.code(), "unknown");
        assert!(!error.to_string().is_empty());
    }
}
