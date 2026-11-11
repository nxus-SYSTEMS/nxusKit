//! CE tool-calling facade for provider-native and user-supplied external tools.
//!
//! tier: ce-impl
//!
//! The surface is intentionally small and limited to CE-safe tool definitions,
//! support queries, registration, and invocation result values.
//! External tool registration performs structural validation of the JSON Schema
//! parameter object. Runtime argument validation remains the provider or
//! user-handler responsibility in v1.1.x.
//!
//! # Examples
//!
//! ```
//! use nxuskit::tools::{ExternalTool, ToolResult};
//! use serde_json::json;
//!
//! let echo = ExternalTool::new(
//!     "echo",
//!     "Echo one JSON object",
//!     json!({ "type": "object" }),
//!     ToolResult::success,
//! );
//!
//! let result = echo.invoke(json!({ "value": "ok" }));
//! assert_eq!(result.output(), &json!({ "value": "ok" }));
//! ```

mod external;
mod provider_native;

// tier: ce-impl
pub use external::{
    CeFacadeError, CeToolPosture, ExternalTool, ToolCallingProvider, ToolCallingSupport,
    ToolResult, register_external_tool, supports_tool_calling, tool_calling_support,
};
