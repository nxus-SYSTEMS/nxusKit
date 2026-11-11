//! Literal catalog/CLI YAML compatibility and fail-closed input regressions.
//! Legacy-reader interoperability is retained separately as development proof.

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde::de::Error as _;
    use serde_json::{Value, json};
    use std::collections::HashMap;

    #[derive(Debug, Deserialize)]
    struct Catalog {
        products: HashMap<String, Product>,
    }
    #[derive(Debug, Deserialize)]
    struct Product {
        editions: HashMap<String, Edition>,
    }
    #[derive(Debug, Deserialize)]
    struct Edition {
        #[serde(default)]
        features: Vec<String>,
        #[serde(default)]
        limits: HashMap<String, serde_yaml_ng::Value>,
        inherits: Option<String>,
    }

    #[test]
    fn catalog_value_api_preserves_typed_limits_and_inheritance() {
        let raw = "products:\n  nxuskit:\n    editions:\n      oss:\n        features: [chat]\n        limits: {max_sessions: 42, max_rules: unlimited}\n      pro:\n        inherits: oss\n        features: [solver]\n        limits: {max_sessions: 100}\n";
        let parsed: Catalog = serde_yaml_ng::from_str(raw).unwrap();
        let editions = &parsed.products["nxuskit"].editions;
        assert_eq!(editions.len(), 2);
        assert_eq!(editions["oss"].features, ["chat"]);
        assert_eq!(editions["oss"].limits["max_sessions"].as_u64(), Some(42));
        assert_eq!(
            editions["oss"].limits["max_rules"].as_str(),
            Some("unlimited")
        );
        assert_eq!(editions["pro"].inherits.as_deref(), Some("oss"));
        assert_eq!(editions["pro"].features, ["solver"]);
        assert_eq!(editions["pro"].limits["max_sessions"].as_u64(), Some(100));
        assert!(matches!(
            editions["oss"].limits["max_sessions"],
            serde_yaml_ng::Value::Number(_, _)
        ));
        assert!(matches!(
            editions["oss"].limits["max_rules"],
            serde_yaml_ng::Value::String(_, _)
        ));
    }

    #[test]
    fn unsigned_limit_boundary_is_not_rounded() {
        let parsed: serde_yaml_ng::Value =
            serde_yaml_ng::from_str("limit: 18446744073709551615\n").unwrap();
        assert_eq!(parsed["limit"].as_u64(), Some(u64::MAX));
    }

    #[test]
    fn emitted_cli_envelope_preserves_literal_json_values() {
        let expected = json!({"ok":true,"result":{"word":"yes","nothing":null,"n":0,"unicode":"雪","rows":[1,2,3]}});
        let wire = serde_yaml_ng::to_string(&expected).unwrap();
        let new: Value = serde_yaml_ng::from_str(&wire).unwrap();
        assert_eq!(new, expected);
    }

    #[test]
    fn literal_pipeline_alias_preserves_structured_json() {
        let raw = "defaults: &base\n  retries: 3\n  enabled: true\nrequest: *base\n";
        let expected =
            json!({"defaults":{"retries":3,"enabled":true},"request":{"retries":3,"enabled":true}});
        assert_eq!(serde_yaml_ng::from_str::<Value>(raw).unwrap(), expected);
    }

    #[test]
    fn malformed_document_is_an_error_not_a_panic() {
        let result =
            std::panic::catch_unwind(|| serde_yaml_ng::from_str::<Value>("items: [unclosed"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn duplicate_keys_fail_closed() {
        assert!(serde_yaml_ng::from_str::<Value>("edition: oss\nedition: pro\n").is_err());
    }

    #[test]
    fn multiple_documents_are_not_silently_ignored() {
        assert!(serde_yaml_ng::from_str::<Value>("a: 1\n---\na: 2\n").is_err());
    }

    #[test]
    fn cli_custom_error_api_remains_available() {
        let error = serde_yaml_ng::Error::custom("literal pipeline error");
        assert!(error.to_string().contains("literal pipeline error"));
    }

    #[test]
    fn typed_catalog_limit_alias_preserves_the_numeric_limit() {
        let raw = "products:\n  nxuskit:\n    editions:\n      oss:\n        limits: {max_sessions: &maximum 42}\n      pro:\n        inherits: oss\n        limits: {max_sessions: *maximum}\n";
        let new: Catalog = serde_yaml_ng::from_str(raw).unwrap();
        assert_eq!(
            new.products["nxuskit"].editions["oss"].limits["max_sessions"].as_u64(),
            Some(42)
        );
        assert_eq!(
            new.products["nxuskit"].editions["pro"].limits["max_sessions"].as_u64(),
            Some(42)
        );
    }

    #[test]
    fn explicit_custom_tags_are_not_silently_erased_for_json_targets() {
        let raw = "limit: !custom 42\n";
        assert!(serde_yaml_ng::from_str::<Value>(raw).is_err());
        let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(raw).unwrap();
        assert!(matches!(parsed["limit"], serde_yaml_ng::Value::Tagged(_)));
    }
}
