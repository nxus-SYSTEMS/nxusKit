//! Integration tests for CLIPS focus-stack control
//!
//! These tests verify that the CLIPS provider correctly supports selective
//! module execution via the `focus` field in JSON input. Focus-stack control
//! allows callers to restrict which modules fire during inference, enabling
//! staged evaluation pipelines and fine-grained control over rule execution.

use nxuskit_engine::LLMProvider;
use nxuskit_engine::providers::ClipsProvider;
use nxuskit_engine::types::{ChatRequest, Message};
use tempfile::TempDir;

// ============================================================================
// Helper: Multi-Module Rule Base
// ============================================================================

/// Create a multi-module rule base for responsive-layout classification.
///
/// Defines two modules:
///   - SCREEN-SIZE: classifies screen dimensions into categories
///   - MENU-LAYOUT: derives menu presentation from screen category
///
/// MENU-LAYOUT imports from SCREEN-SIZE, forming a two-stage pipeline.
fn create_multimodule_rules(dir: &std::path::Path) -> std::io::Result<()> {
    let rules = r#"
;;; Module: SCREEN-SIZE
;;; Classifies viewport dimensions into responsive categories.

(defmodule SCREEN-SIZE (export ?ALL))

(deftemplate SCREEN-SIZE::screen-input
    (slot width (type INTEGER))
    (slot height (type INTEGER)))

(deftemplate SCREEN-SIZE::screen-config
    (slot category (type SYMBOL))
    (slot columns (type INTEGER)))

(defrule SCREEN-SIZE::classify-mobile
    "Narrow viewport is mobile: single column layout"
    (screen-input (width ?w&:(< ?w 768)))
    =>
    (assert (screen-config (category mobile) (columns 1))))

(defrule SCREEN-SIZE::classify-desktop
    "Wide viewport is desktop: three column layout"
    (screen-input (width ?w&:(>= ?w 1024)))
    =>
    (assert (screen-config (category desktop) (columns 3))))

;;; Module: MENU-LAYOUT
;;; Derives menu presentation style from screen category.

(defmodule MENU-LAYOUT (import SCREEN-SIZE ?ALL) (export ?ALL))

(deftemplate MENU-LAYOUT::menu-state
    (slot style (type SYMBOL))
    (slot max-depth (type INTEGER)))

(defrule MENU-LAYOUT::mobile-hamburger
    "Mobile screens use hamburger menu with shallow depth"
    (screen-config (category mobile))
    =>
    (assert (menu-state (style hamburger) (max-depth 2))))

(defrule MENU-LAYOUT::desktop-horizontal
    "Desktop screens use horizontal menu with deep nesting"
    (screen-config (category desktop))
    =>
    (assert (menu-state (style horizontal) (max-depth 4))))
"#;

    std::fs::write(dir.join("responsive-layout.clp"), rules)?;
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

/// Test that focusing on a single module restricts which rules fire.
///
/// When focus is set to ["SCREEN-SIZE"], only the screen classification
/// rules should execute. The MENU-LAYOUT module should not fire, so
/// no menu-state conclusions should appear.
#[tokio::test]
async fn test_focus_single_module() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_multimodule_rules(temp_dir.path()).expect("Should create rules");

    let clips = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .persistent(true)
        .build()
        .expect("Should build");

    let input = r#"{"focus": ["SCREEN-SIZE"], "facts": [{"template": "screen-input", "values": {"width": 375, "height": 812}}]}"#;

    let request = ChatRequest::new("responsive-layout.clp").with_message(Message::user(input));

    let response = clips.chat(&request).await.expect("Should succeed");
    let output: serde_json::Value =
        serde_json::from_str(&response.content).expect("Should parse JSON");

    let conclusions = output
        .get("conclusions")
        .and_then(|c| c.as_array())
        .expect("Should have conclusions array");

    // Should have screen-config conclusion (mobile, 1 column)
    let has_screen_config = conclusions
        .iter()
        .any(|c| c.get("template").and_then(|t| t.as_str()) == Some("screen-config"));
    assert!(
        has_screen_config,
        "Should have screen-config conclusion from SCREEN-SIZE module"
    );

    // Should NOT have menu-state conclusion (MENU-LAYOUT was not focused)
    let has_menu_state = conclusions
        .iter()
        .any(|c| c.get("template").and_then(|t| t.as_str()) == Some("menu-state"));
    assert!(
        !has_menu_state,
        "Should NOT have menu-state conclusion when only SCREEN-SIZE is focused"
    );
}

/// Test that focusing multiple modules in sequence produces a pipeline.
///
/// First chat focuses on SCREEN-SIZE to derive screen-config. Because
/// persistent mode is on, screen-config persists. The second chat
/// then focuses on MENU-LAYOUT, which reads the persisted screen-config
/// and derives menu-state.
#[tokio::test]
async fn test_focus_multiple_modules_ordered() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_multimodule_rules(temp_dir.path()).expect("Should create rules");

    let clips = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .persistent(true)
        .build()
        .expect("Should build");

    // Step 1: Focus on SCREEN-SIZE to derive screen-config
    let input1 = r#"{"focus": ["SCREEN-SIZE"], "facts": [{"template": "screen-input", "values": {"width": 375, "height": 812}}]}"#;

    let request1 = ChatRequest::new("responsive-layout.clp").with_message(Message::user(input1));

    let response1 = clips
        .chat(&request1)
        .await
        .expect("First chat should succeed");
    let output1: serde_json::Value =
        serde_json::from_str(&response1.content).expect("Should parse JSON");

    let conclusions1 = output1
        .get("conclusions")
        .and_then(|c| c.as_array())
        .expect("Should have conclusions");

    // Verify screen-config was derived
    let has_screen_config = conclusions1
        .iter()
        .any(|c| c.get("template").and_then(|t| t.as_str()) == Some("screen-config"));
    assert!(has_screen_config, "First chat should derive screen-config");

    // Step 2: Focus on MENU-LAYOUT. screen-config persists from step 1.
    let input2 = r#"{"focus": ["MENU-LAYOUT"], "facts": []}"#;

    let request2 = ChatRequest::new("responsive-layout.clp").with_message(Message::user(input2));

    let response2 = clips
        .chat(&request2)
        .await
        .expect("Second chat should succeed");
    let output2: serde_json::Value =
        serde_json::from_str(&response2.content).expect("Should parse JSON");

    let conclusions2 = output2
        .get("conclusions")
        .and_then(|c| c.as_array())
        .expect("Should have conclusions");

    // Verify menu-state was derived from the persisted screen-config
    let has_menu_state = conclusions2
        .iter()
        .any(|c| c.get("template").and_then(|t| t.as_str()) == Some("menu-state"));
    assert!(
        has_menu_state,
        "Second chat should derive menu-state from persisted screen-config"
    );
}

/// Test that focusing on a nonexistent module returns an error.
///
/// The error message should mention the available modules so the caller
/// can correct the request.
#[tokio::test]
async fn test_focus_invalid_module_error() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_multimodule_rules(temp_dir.path()).expect("Should create rules");

    let clips = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .build()
        .expect("Should build");

    let input = r#"{"focus": ["NONEXISTENT"], "facts": [{"template": "screen-input", "values": {"width": 375, "height": 812}}]}"#;

    let request = ChatRequest::new("responsive-layout.clp").with_message(Message::user(input));

    let result = clips.chat(&request).await;

    assert!(
        result.is_err(),
        "Focusing on a nonexistent module should return an error"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("NONEXISTENT"),
        "Error should mention the invalid module name: {}",
        err_msg
    );

    // The error should list available modules so the caller can self-correct
    let err_lower = err_msg.to_lowercase();
    assert!(
        err_lower.contains("available") || err_lower.contains("module"),
        "Error should mention available modules: {}",
        err_msg
    );
}

/// Test that omitting the focus field runs all modules (default behavior).
///
/// Without a focus constraint, both SCREEN-SIZE and MENU-LAYOUT should
/// fire, producing both screen-config and menu-state conclusions.
#[tokio::test]
async fn test_no_focus_runs_all_modules() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_multimodule_rules(temp_dir.path()).expect("Should create rules");

    let clips = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .build()
        .expect("Should build");

    // No "focus" field -- should run all modules
    let input =
        r#"{"facts": [{"template": "screen-input", "values": {"width": 375, "height": 812}}]}"#;

    let request = ChatRequest::new("responsive-layout.clp").with_message(Message::user(input));

    let response = clips.chat(&request).await.expect("Should succeed");
    let output: serde_json::Value =
        serde_json::from_str(&response.content).expect("Should parse JSON");

    let conclusions = output
        .get("conclusions")
        .and_then(|c| c.as_array())
        .expect("Should have conclusions");

    // Should have screen-config from SCREEN-SIZE module
    let has_screen_config = conclusions
        .iter()
        .any(|c| c.get("template").and_then(|t| t.as_str()) == Some("screen-config"));
    assert!(
        has_screen_config,
        "Should have screen-config when running all modules"
    );

    // Should have menu-state from MENU-LAYOUT module
    let has_menu_state = conclusions
        .iter()
        .any(|c| c.get("template").and_then(|t| t.as_str()) == Some("menu-state"));
    assert!(
        has_menu_state,
        "Should have menu-state when running all modules"
    );
}

/// Test backward compatibility: JSON with no focus field works identically
/// to pre-focus behavior.
///
/// This ensures that existing callers who never send a focus field
/// continue to work without changes.
#[tokio::test]
async fn test_focus_backward_compatible() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_multimodule_rules(temp_dir.path()).expect("Should create rules");

    let clips = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .build()
        .expect("Should build");

    // Input with only facts, no focus field at all (pre-focus format)
    let input =
        r#"{"facts": [{"template": "screen-input", "values": {"width": 1280, "height": 720}}]}"#;

    let request = ChatRequest::new("responsive-layout.clp").with_message(Message::user(input));

    let response = clips.chat(&request).await.expect("Should succeed");
    let output: serde_json::Value =
        serde_json::from_str(&response.content).expect("Should parse JSON");

    let conclusions = output
        .get("conclusions")
        .and_then(|c| c.as_array())
        .expect("Should have conclusions");

    // Desktop viewport (1280 >= 1024) should produce both screen-config and menu-state
    let has_screen_config = conclusions
        .iter()
        .any(|c| c.get("template").and_then(|t| t.as_str()) == Some("screen-config"));
    assert!(
        has_screen_config,
        "Backward-compatible input should still derive screen-config"
    );

    // Verify the desktop classification produced correct values
    let screen_config = conclusions
        .iter()
        .find(|c| c.get("template").and_then(|t| t.as_str()) == Some("screen-config"))
        .expect("Should find screen-config conclusion");

    let category = screen_config
        .get("values")
        .and_then(|v| v.get("category"))
        .and_then(|c| {
            c.get("symbol")
                .and_then(|s| s.as_str())
                .or_else(|| c.as_str())
        });
    assert_eq!(
        category,
        Some("desktop"),
        "1280px width should classify as desktop"
    );

    let has_menu_state = conclusions
        .iter()
        .any(|c| c.get("template").and_then(|t| t.as_str()) == Some("menu-state"));
    assert!(
        has_menu_state,
        "Backward-compatible input should still derive menu-state"
    );
}
