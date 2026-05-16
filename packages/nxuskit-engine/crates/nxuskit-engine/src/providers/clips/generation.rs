//! Claude-assisted CLIPS rulebase generation
//!
//! This module provides functionality for generating CLIPS rulebases
//! from natural language descriptions using LLMs.

use crate::error::{NxuskitError, Result};
use crate::provider::LLMProvider;
use crate::types::{ChatRequest, Message};

use super::types::*;

use std::sync::Arc;

// ============================================================================
// Generation Prompts
// ============================================================================

/// System prompt for CLIPS rule generation
pub const CLIPS_SYSTEM_PROMPT: &str = r#"You are an expert CLIPS rule developer. Generate syntactically correct CLIPS 6.4 code following these guidelines:

## TEMPLATES (deftemplate)
- Use descriptive slot names in kebab-case
- Always specify slot types: INTEGER, FLOAT, STRING, SYMBOL, FACT-ADDRESS
- Use (default ...) for optional slots with sensible defaults
- Use (allowed-symbols ...) or (allowed-values ...) for constrained slots
- Add documentation strings describing the template's purpose
- Use multislot for lists of values

## RULES (defrule)
- Use descriptive rule names in kebab-case
- Add documentation strings explaining when and why the rule fires
- Use pattern variables (?var) with meaningful names (?customer, ?age, not ?x)
- Use constraint patterns (&:(condition)) for numeric comparisons
- Use (not ...) patterns to prevent duplicate conclusions
- Consider rule ordering with salience when priority matters
- Avoid rules that re-trigger themselves (infinite loops)

## STYLE
- Indent consistently (2 spaces)
- Group related constructs with section comments (;;; Section Name)
- Add inline comments for complex patterns
- Keep rules focused on a single logical decision

## SAFETY
- Never create rules that can fire infinitely
- Use (exists ...) and (forall ...) patterns carefully
- Validate slot values match expected types
- Handle edge cases explicitly

Output ONLY valid CLIPS code. Do not include explanations unless specifically requested.
Wrap code in ```clips code blocks."#;

/// Prompt template for policy document conversion
pub const POLICY_PROMPT_TEMPLATE: &str = r#"Convert the following policy document into CLIPS rules.

POLICY DOCUMENT:
---
{policy}
---

Requirements:
1. Create deftemplates for all entities mentioned in the policy
2. Create defrules for each policy statement or business rule
3. Handle edge cases mentioned or implied in the policy
4. Add documentation strings to all constructs
5. Use appropriate slot types and constraints

Output complete, valid CLIPS code:"#;

/// Prompt template for JSON specification conversion
pub const JSON_SPEC_PROMPT_TEMPLATE: &str = r#"Generate CLIPS code from this JSON specification:

```json
{spec}
```

Requirements:
1. Create all specified templates with exact slot definitions
2. Implement all specified rules with correct pattern matching
3. Add helper rules if needed for completeness
4. Validate slot types and constraints match the specification
5. Include documentation for all constructs

Output complete, valid CLIPS code:"#;

/// Prompt template for decision table conversion
pub const DECISION_TABLE_PROMPT_TEMPLATE: &str = r#"Convert this decision table into CLIPS rules:

{table}

Requirements:
1. Create an input template with all condition columns as slots
2. Create an output/decision template with all action columns as slots
3. Generate one defrule per table row
4. Handle "any", "*", or empty cells as wildcards (no constraint)
5. Name rules descriptively based on the row conditions
6. Add (not ...) patterns to prevent duplicate decisions

Output complete, valid CLIPS code:"#;

/// Prompt template for example-based generation
pub const EXAMPLE_PROMPT_TEMPLATE: &str = r#"Based on these input/output examples, generate CLIPS rules that produce the same outputs:

{examples}

Requirements:
1. Infer the necessary templates from the example data
2. Create rules that match the input patterns and produce the outputs
3. Generalize from specific examples to handle similar cases
4. Add documentation explaining the inferred logic
5. Handle edge cases that might arise

Output complete, valid CLIPS code:"#;

/// Prompt template for test case generation
pub const TEST_GENERATION_PROMPT_TEMPLATE: &str = r#"For the following CLIPS rules, generate comprehensive test cases:

```clips
{code}
```

Generate test cases in JSON format covering:
1. Normal cases where rules should fire
2. Boundary cases (edge of numeric conditions)
3. Negative cases where rules should NOT fire
4. Combination cases testing multiple rules together

Output format:
```json
{
  "test_cases": [
    {
      "name": "descriptive_test_name",
      "description": "What this test verifies",
      "input_facts": [
        {"template": "...", "values": {...}}
      ],
      "expected_conclusions": [
        {"template": "...", "values": {...}}
      ],
      "expected_rules": ["rule-name-1", "rule-name-2"]
    }
  ]
}
```"#;

// ============================================================================
// Generator
// ============================================================================

/// CLIPS rulebase generator using LLMs
pub struct RulebaseGenerator {
    /// The LLM provider to use for generation
    provider: Arc<dyn LLMProvider>,

    /// Model to use for generation
    model: String,

    /// Default generation options
    default_options: GenerationOptions,
}

impl std::fmt::Debug for RulebaseGenerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RulebaseGenerator")
            .field("model", &self.model)
            .field("default_options", &self.default_options)
            .finish_non_exhaustive()
    }
}

impl RulebaseGenerator {
    /// Create a new generator with the given LLM provider
    pub fn new(provider: Arc<dyn LLMProvider>, model: impl Into<String>) -> Self {
        Self {
            provider,
            model: model.into(),
            default_options: GenerationOptions::default(),
        }
    }

    /// Set default generation options
    pub fn with_options(mut self, options: GenerationOptions) -> Self {
        self.default_options = options;
        self
    }

    /// Generate a rulebase from a natural language policy document
    pub async fn from_policy(&self, policy: &str) -> Result<GeneratedRulebase> {
        let prompt = POLICY_PROMPT_TEMPLATE.replace("{policy}", policy);
        self.generate(&prompt, "policy_document").await
    }

    /// Generate a rulebase from a JSON specification
    pub async fn from_json_spec(&self, spec: &serde_json::Value) -> Result<GeneratedRulebase> {
        let spec_str = serde_json::to_string_pretty(spec).map_err(NxuskitError::Serialization)?;
        let prompt = JSON_SPEC_PROMPT_TEMPLATE.replace("{spec}", &spec_str);
        self.generate(&prompt, "json_specification").await
    }

    /// Generate a rulebase from a decision table (markdown format)
    pub async fn from_decision_table(&self, table: &str) -> Result<GeneratedRulebase> {
        let prompt = DECISION_TABLE_PROMPT_TEMPLATE.replace("{table}", table);
        self.generate(&prompt, "decision_table").await
    }

    /// Generate a rulebase from input/output examples
    pub async fn from_examples(&self, examples: &str) -> Result<GeneratedRulebase> {
        let prompt = EXAMPLE_PROMPT_TEMPLATE.replace("{examples}", examples);
        self.generate(&prompt, "examples").await
    }

    /// Generate test cases for existing CLIPS code
    pub async fn generate_tests(&self, clips_code: &str) -> Result<Vec<GeneratedTestCase>> {
        let prompt = TEST_GENERATION_PROMPT_TEMPLATE.replace("{code}", clips_code);

        let request = ChatRequest::new(&self.model)
            .with_message(Message::system(CLIPS_SYSTEM_PROMPT))
            .with_message(Message::user(&prompt));

        let response = self.provider.chat(&request).await?;
        let json_str = extract_json(&response.content)?;

        #[derive(serde::Deserialize)]
        struct TestCasesWrapper {
            test_cases: Vec<GeneratedTestCase>,
        }

        let wrapper: TestCasesWrapper = serde_json::from_str(&json_str).map_err(|e| {
            NxuskitError::InvalidRequest(format!("Failed to parse test cases: {}", e))
        })?;

        Ok(wrapper.test_cases)
    }

    /// Core generation method
    async fn generate(&self, prompt: &str, source_type: &str) -> Result<GeneratedRulebase> {
        let start = std::time::Instant::now();

        let request = ChatRequest::new(&self.model)
            .with_message(Message::system(CLIPS_SYSTEM_PROMPT))
            .with_message(Message::user(prompt));

        let response = self.provider.chat(&request).await?;

        // Extract CLIPS code from response
        let code = extract_clips_code(&response.content)?;

        // Count constructs
        let template_count = code.matches("(deftemplate").count();
        let rule_count = code.matches("(defrule").count();

        // Validate if requested
        let validation = match self.default_options.validation {
            ValidationLevel::None => ValidationResult::skipped(),
            ValidationLevel::Syntax => self.validate_syntax(&code),
            ValidationLevel::Semantic => self.validate_semantic(&code),
            ValidationLevel::Full => self.validate_full(&code).await?,
        };

        // Generate tests if requested
        let tests = if self.default_options.include_tests && validation.passed {
            self.generate_tests(&code).await.unwrap_or_default()
        } else {
            vec![]
        };

        Ok(GeneratedRulebase {
            code,
            validation,
            tests,
            metadata: GenerationMetadata {
                source_type: source_type.to_string(),
                generator_model: self.model.clone(),
                generated_at: chrono::Utc::now().to_rfc3339(),
                template_count,
                rule_count,
                generation_time_ms: start.elapsed().as_millis() as u64,
            },
        })
    }

    /// Validate CLIPS syntax (basic check)
    fn validate_syntax(&self, code: &str) -> ValidationResult {
        let mut errors = Vec::new();

        // Check for balanced parentheses
        let mut depth = 0;
        let mut in_string = false;
        let mut escape = false;

        for (i, ch) in code.char_indices() {
            if escape {
                escape = false;
                continue;
            }

            match ch {
                '\\' if in_string => escape = true,
                '"' => in_string = !in_string,
                '(' if !in_string => depth += 1,
                ')' if !in_string => {
                    depth -= 1;
                    if depth < 0 {
                        errors.push(ValidationError {
                            code: "UNBALANCED_PARENS".to_string(),
                            message: "Unexpected closing parenthesis".to_string(),
                            line: Some(code[..i].lines().count()),
                            construct: None,
                        });
                    }
                }
                _ => {}
            }
        }

        if depth != 0 {
            errors.push(ValidationError {
                code: "UNBALANCED_PARENS".to_string(),
                message: format!("Unbalanced parentheses: {} unclosed", depth),
                line: None,
                construct: None,
            });
        }

        // Check for required constructs
        if !code.contains("(deftemplate") && !code.contains("(defrule") {
            errors.push(ValidationError {
                code: "NO_CONSTRUCTS".to_string(),
                message: "No deftemplates or defrules found".to_string(),
                line: None,
                construct: None,
            });
        }

        if errors.is_empty() {
            ValidationResult::passed()
        } else {
            ValidationResult::failed(errors)
        }
    }

    /// Validate CLIPS semantics (check template references, etc.)
    fn validate_semantic(&self, code: &str) -> ValidationResult {
        let mut result = self.validate_syntax(code);

        if !result.passed {
            return result;
        }

        // Extract template names
        let template_names: std::collections::HashSet<_> = code
            .split("(deftemplate")
            .skip(1)
            .filter_map(|s| s.split_whitespace().next())
            .collect();

        // Check that rules reference existing templates
        for rule_section in code.split("(defrule").skip(1) {
            // Find patterns in the rule (simplified check)
            for line in rule_section.lines() {
                let line = line.trim();
                if line.starts_with('(')
                    && !line.starts_with("(declare")
                    && !line.starts_with("(test")
                    && !line.starts_with("(not")
                    && !line.starts_with("(or")
                    && !line.starts_with("(and")
                    && !line.starts_with("(exists")
                    && !line.starts_with("(forall")
                    && !line.starts_with("(assert")
                    && !line.starts_with("(modify")
                    && !line.starts_with("(retract")
                    && !line.starts_with("(printout")
                    && !line.starts_with("(bind")
                    && !line.starts_with("(if")
                    && let Some(template_name) = line
                        .trim_start_matches('(')
                        .split(|c: char| c.is_whitespace() || c == '(')
                        .next()
                    && !template_name.is_empty()
                    && !template_name.starts_with('?')
                    && !template_names.contains(template_name)
                    && template_name != "initial-fact"
                {
                    // This might be a pattern - extract template name
                    result.warnings.push(ValidationWarning {
                        code: "UNKNOWN_TEMPLATE".to_string(),
                        message: format!("Template '{}' not defined in this file", template_name),
                        location: Some(line.to_string()),
                    });
                }
            }
        }

        result
    }

    /// Full validation with CLIPS environment
    async fn validate_full(&self, code: &str) -> Result<ValidationResult> {
        use clips_sys::ClipsEnvironment;

        let mut result = self.validate_semantic(code);

        if !result.passed {
            return Ok(result);
        }

        // Try to load in CLIPS environment
        let env = ClipsEnvironment::new().map_err(|e| {
            NxuskitError::Configuration(format!("Failed to create CLIPS environment: {}", e))
        })?;

        match env.load_from_string(code) {
            Ok(()) => {
                // Check for common issues
                let template_count = env.templates().count();
                let rule_count = env.rules().count();

                if template_count == 0 && rule_count == 0 {
                    result.warnings.push(ValidationWarning {
                        code: "NO_LOADED_CONSTRUCTS".to_string(),
                        message: "No constructs were loaded into CLIPS".to_string(),
                        location: None,
                    });
                }
            }
            Err(e) => {
                result.passed = false;
                result.errors.push(ValidationError {
                    code: "CLIPS_LOAD_ERROR".to_string(),
                    message: format!("CLIPS failed to load: {}", e),
                    line: None,
                    construct: None,
                });
            }
        }

        Ok(result)
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Extract CLIPS code from a response that may contain markdown
fn extract_clips_code(response: &str) -> Result<String> {
    // Try to find clips code block
    if let Some(start) = response.find("```clips") {
        let code_start = start + 8;
        if let Some(end) = response[code_start..].find("```") {
            return Ok(response[code_start..code_start + end].trim().to_string());
        }
    }

    // Try generic code block
    if let Some(start) = response.find("```") {
        let code_start = response[start + 3..]
            .find('\n')
            .map(|i| start + 4 + i)
            .unwrap_or(start + 3);
        if let Some(end) = response[code_start..].find("```") {
            return Ok(response[code_start..code_start + end].trim().to_string());
        }
    }

    // Check if the response itself looks like CLIPS code
    if response.contains("(deftemplate") || response.contains("(defrule") {
        return Ok(response.trim().to_string());
    }

    Err(NxuskitError::InvalidRequest(
        "No CLIPS code found in response".to_string(),
    ))
}

/// Extract JSON from a response that may contain markdown
fn extract_json(response: &str) -> Result<String> {
    // Try to find json code block
    if let Some(start) = response.find("```json") {
        let code_start = start + 7;
        if let Some(end) = response[code_start..].find("```") {
            return Ok(response[code_start..code_start + end].trim().to_string());
        }
    }

    // Try generic code block
    if let Some(start) = response.find("```") {
        let code_start = response[start + 3..]
            .find('\n')
            .map(|i| start + 4 + i)
            .unwrap_or(start + 3);
        if let Some(end) = response[code_start..].find("```") {
            let content = response[code_start..code_start + end].trim();
            // Verify it looks like JSON
            if content.starts_with('{') || content.starts_with('[') {
                return Ok(content.to_string());
            }
        }
    }

    // Check if the response itself is JSON
    let trimmed = response.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Ok(trimmed.to_string());
    }

    Err(NxuskitError::InvalidRequest(
        "No JSON found in response".to_string(),
    ))
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Generate CLIPS rules from a policy document using the provided LLM
pub async fn generate_from_policy(
    provider: Arc<dyn LLMProvider>,
    model: &str,
    policy: &str,
) -> Result<GeneratedRulebase> {
    let generator = RulebaseGenerator::new(provider, model);
    generator.from_policy(policy).await
}

/// Generate CLIPS rules from a JSON specification using the provided LLM
pub async fn generate_from_spec(
    provider: Arc<dyn LLMProvider>,
    model: &str,
    spec: &serde_json::Value,
) -> Result<GeneratedRulebase> {
    let generator = RulebaseGenerator::new(provider, model);
    generator.from_json_spec(spec).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_clips_code() {
        let response = r#"Here is the CLIPS code:

```clips
(deftemplate test
  (slot value))

(defrule test-rule
  (test (value ?v))
  =>
  (printout t "Value: " ?v crlf))
```

This implements the requested rules."#;

        let code = extract_clips_code(response).unwrap();
        assert!(code.contains("(deftemplate test"));
        assert!(code.contains("(defrule test-rule"));
    }

    #[test]
    fn test_extract_json() {
        let response = r#"Here are the test cases:

```json
{
  "test_cases": [
    {"name": "test1", "description": "A test"}
  ]
}
```"#;

        let json = extract_json(response).unwrap();
        assert!(json.contains("test_cases"));
    }

    #[test]
    fn test_validate_syntax_balanced() {
        let generator = RulebaseGenerator::new(
            Arc::new(crate::providers::MockProvider::new("test")),
            "test",
        );

        let code = "(deftemplate test (slot x))";
        let result = generator.validate_syntax(code);
        assert!(result.passed);
    }

    #[test]
    fn test_validate_syntax_unbalanced() {
        let generator = RulebaseGenerator::new(
            Arc::new(crate::providers::MockProvider::new("test")),
            "test",
        );

        let code = "(deftemplate test (slot x)";
        let result = generator.validate_syntax(code);
        assert!(!result.passed);
    }
}
