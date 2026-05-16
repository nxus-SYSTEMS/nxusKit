//! MCP (Model Context Protocol) provider implementation
//!
//! # Important: MCP Architecture
//!
//! **MCP servers are NOT LLM providers**. They provide tools, resources, and prompts
//! that augment LLM capabilities. MCP servers do not directly provide chat/completion
//! endpoints.
//!
//! ## MCP Core Primitives
//!
//! - **Tools**: Functions that AI models can execute
//! - **Resources**: Context and data for AI models to use
//! - **Prompts**: Templated messages and workflows
//! - **Sampling**: MCP servers can request the client to perform LLM sampling
//!
//! ## Usage Pattern
//!
//! MCP providers should be used to discover and invoke tools/resources that
//! complement a primary LLM provider (Claude, OpenAI, etc.):
//!
//! ```no_run
//! use nxuskit_engine::prelude::*;
//!
//! # async fn example() -> Result<()> {
//!     // Create MCP provider to access tools/resources
//!     let mcp = McpProvider::builder()
//!         .server_uri("stdio://mcp-server")
//!         .model_name("mcp-tools")
//!         .build()?;
//!
//!     // Discover available MCP capabilities
//!     let tools = mcp.list_tools().await?;
//!     println!("Available tools: {:?}", tools);
//!
//!     // Use with a real LLM provider for chat
//!     let api_key = std::env::var("ANTHROPIC_API_KEY")
//!         .unwrap_or_else(|_| "sk-test".to_string());
//!     let claude = ClaudeProvider::builder()
//!         .api_key(api_key)
//!         .build()?;
//!
//!     let response = claude.chat(&ChatRequest::new("claude-3-5-sonnet-20241022")
//!         .with_message(Message::user("Hello!"))).await?;
//!
//!     Ok(())
//! # }
//! ```

use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::{
    ChatRequest, ChatResponse, LLMProvider, ModelInfo, StreamChunk,
    error::{NxuskitError, Result},
    types::ProviderCapabilities,
};

/// Information about an MCP tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: Option<String>,
    /// Input schema for the tool
    pub input_schema: Option<serde_json::Value>,
}

/// Information about an MCP resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceInfo {
    /// Resource URI
    pub uri: String,
    /// Resource name
    pub name: String,
    /// Resource description
    pub description: Option<String>,
    /// MIME type
    pub mime_type: Option<String>,
}

/// Result of calling an MCP tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    /// Result content
    pub content: Vec<McpContent>,
    /// Whether the tool call resulted in an error
    pub is_error: bool,
}

/// Content returned from MCP operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpContent {
    /// Text content
    #[serde(rename = "text")]
    Text { text: String },
    /// Image content
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    /// Resource content
    #[serde(rename = "resource")]
    Resource { uri: String, text: Option<String> },
}

/// Default timeout for MCP operations
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// MCP server configuration from servers.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Server name (from the JSON key)
    pub name: String,
    /// Command to execute
    pub command: String,
    /// Command arguments
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

/// MCP servers configuration file format
#[derive(Debug, Deserialize)]
struct McpConfig {
    servers: std::collections::HashMap<String, McpServerEntry>,
}

/// Individual server entry in the config
#[derive(Debug, Deserialize)]
struct McpServerEntry {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: std::collections::HashMap<String, String>,
}

/// MCP provider for Model Context Protocol servers
///
/// This provider connects to MCP servers via various transports (STDIO, TCP)
/// and provides a unified interface for chat operations.
pub struct McpProvider {
    /// URI of the MCP server (e.g., "stdio://command", "tcp://host:port")
    server_uri: String,
    /// Optional authentication token
    #[allow(dead_code)] // Will be used when implementing MCP connection logic
    auth_token: Option<String>,
    /// Model name to report in responses
    model_name: String,
    /// Timeout for MCP operations
    timeout: Duration,
}

impl McpProvider {
    /// Create a new MCP provider builder
    pub fn builder() -> McpProviderBuilder {
        McpProviderBuilder::default()
    }

    /// Discover MCP servers from configuration file
    ///
    /// Searches for MCP server configurations in the following order:
    /// 1. Provided `config_path` parameter
    /// 2. `NXUSKIT_MCP_CONFIG` environment variable (or legacy `RUSTYLLM_MCP_CONFIG`)
    /// 3. `~/.config/mcp/servers.json` (standard location)
    ///
    /// Returns an empty vector if no configuration is found.
    pub fn discover_servers(
        config_path: Option<std::path::PathBuf>,
    ) -> Result<Vec<McpServerConfig>> {
        use std::fs;
        use std::path::PathBuf;

        // Determine config file location
        let path = if let Some(p) = config_path {
            p
        } else if let Ok(env_path) =
            std::env::var("NXUSKIT_MCP_CONFIG").or_else(|_| std::env::var("RUSTYLLM_MCP_CONFIG"))
        {
            PathBuf::from(env_path)
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".config/mcp/servers.json")
        } else {
            // No config found
            return Ok(Vec::new());
        };

        // If file doesn't exist, return empty (not an error)
        if !path.exists() {
            return Ok(Vec::new());
        }

        // Read and parse config file
        let content = fs::read_to_string(&path).map_err(|e| {
            NxuskitError::Configuration(format!(
                "Failed to read MCP config file {}: {}",
                path.display(),
                e
            ))
        })?;

        let config: McpConfig = serde_json::from_str(&content).map_err(|e| {
            NxuskitError::Configuration(format!("Failed to parse MCP config: {}", e))
        })?;

        // Convert to McpServerConfig format
        let servers: Vec<McpServerConfig> = config
            .servers
            .into_iter()
            .map(|(name, entry)| McpServerConfig {
                name,
                command: entry.command,
                args: entry.args,
                env: entry.env,
            })
            .collect();

        // Validate each server
        for server in &servers {
            if server.command.is_empty() {
                return Err(NxuskitError::Configuration(format!(
                    "MCP server '{}' has empty command",
                    server.name
                )));
            }
        }

        Ok(servers)
    }

    /// Create an McpProvider from a server configuration
    ///
    /// Constructs a stdio:// URI from the server's command and args.
    pub fn from_server_config(config: &McpServerConfig) -> Result<Self> {
        // For now, we construct a stdio URI from the command
        // In future, we might support other transports based on config
        let server_uri = format!("stdio://{}", config.command);

        Self::builder()
            .server_uri(server_uri)
            .model_name(&config.name)
            .build()
    }

    /// List available tools from the MCP server
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nxuskit_engine::prelude::*;
    /// # async fn example() -> Result<Vec<McpToolInfo>> {
    /// let provider = McpProvider::builder()
    ///     .server_uri("stdio://mcp-server")
    ///     .build()?;
    ///
    /// let tools = provider.list_tools().await?;
    /// for tool in &tools {
    ///     println!("Tool: {} - {:?}", tool.name, tool.description);
    /// }
    /// # Ok(tools)
    /// # }
    /// ```
    pub async fn list_tools(&self) -> Result<Vec<McpToolInfo>> {
        Err(NxuskitError::not_implemented("MCP tool listing"))
    }

    /// Call an MCP tool with the given arguments
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nxuskit_engine::prelude::*;
    /// # use serde_json::json;
    /// # async fn example() -> Result<McpToolResult> {
    /// let provider = McpProvider::builder()
    ///     .server_uri("stdio://mcp-server")
    ///     .build()?;
    ///
    /// let args = json!({
    ///     "query": "rust async programming"
    /// });
    ///
    /// let result = provider.call_tool("search", args.as_object().cloned()).await?;
    /// println!("Tool result: {:?}", result);
    /// # Ok(result)
    /// # }
    /// ```
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<McpToolResult> {
        let _ = (name, arguments); // Avoid unused variable warnings
        Err(NxuskitError::not_implemented("MCP tool calling"))
    }

    /// List available resources from the MCP server
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nxuskit_engine::prelude::*;
    /// # async fn example() -> Result<Vec<McpResourceInfo>> {
    /// let provider = McpProvider::builder()
    ///     .server_uri("stdio://mcp-server")
    ///     .build()?;
    ///
    /// let resources = provider.list_resources().await?;
    /// for resource in &resources {
    ///     println!("Resource: {} - {}", resource.name, resource.uri);
    /// }
    /// # Ok(resources)
    /// # }
    /// ```
    pub async fn list_resources(&self) -> Result<Vec<McpResourceInfo>> {
        Err(NxuskitError::not_implemented("MCP resource listing"))
    }

    /// Get content from an MCP resource
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nxuskit_engine::prelude::*;
    /// # async fn example() -> Result<Vec<McpContent>> {
    /// let provider = McpProvider::builder()
    ///     .server_uri("stdio://mcp-server")
    ///     .build()?;
    ///
    /// let content = provider.get_resource("file://path/to/document").await?;
    /// println!("Resource content: {:?}", content);
    /// # Ok(content)
    /// # }
    /// ```
    pub async fn get_resource(&self, uri: &str) -> Result<Vec<McpContent>> {
        let _ = uri; // Avoid unused variable warning
        Err(NxuskitError::not_implemented("MCP resource access"))
    }
}

impl std::fmt::Debug for McpProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpProvider")
            .field("server_uri", &self.server_uri)
            .field("model_name", &self.model_name)
            .field("timeout", &self.timeout)
            .field("auth_token", &"[REDACTED]")
            .finish()
    }
}

#[async_trait]
impl LLMProvider for McpProvider {
    async fn chat(&self, _request: &ChatRequest) -> Result<ChatResponse> {
        Err(NxuskitError::not_implemented("MCP chat"))
    }

    async fn chat_stream(
        &self,
        _request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        let error_stream = stream! {
            yield Err(NxuskitError::not_implemented("MCP chat streaming"));
        };

        Ok(Box::new(Box::pin(error_stream)))
    }

    fn provider_name(&self) -> &str {
        "mcp"
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Return a single pseudo-model representing this MCP server's capabilities
        let mut info = ModelInfo::new(&self.model_name);
        info.description = Some(format!(
            "MCP server at {}. Provides tools, resources, and prompts (not chat/completion).",
            self.server_uri
        ));

        // Add MCP-specific metadata
        info.metadata
            .insert("provider".to_string(), "mcp".to_string());
        info.metadata
            .insert("modalities".to_string(), "tools,resources".to_string());
        info.metadata.insert(
            "capabilities".to_string(),
            "tools,resources,prompts".to_string(),
        );
        info.metadata
            .insert("server_uri".to_string(), self.server_uri.clone());
        info.metadata.insert(
            "note".to_string(),
            "Use list_tools() and call_tool() methods for MCP interaction".to_string(),
        );

        Ok(vec![info])
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_system_messages: false,
            supports_streaming: false,
            supports_vision: false,
            max_stop_sequences: None,
            supports_presence_penalty: false,
            supports_frequency_penalty: false,
            supports_seed: false,
            supports_logprobs: false,

            supports_streaming_logprobs: false,
            supports_json_mode: false,
            supports_json_schema: false,
            penalty_range: None,
            max_logprobs: None,
        }
    }
}

/// Builder for configuring an MCP provider
#[derive(Debug, Default)]
pub struct McpProviderBuilder {
    server_uri: Option<String>,
    auth_token: Option<String>,
    model_name: Option<String>,
    timeout: Option<Duration>,
}

impl McpProviderBuilder {
    /// Set the MCP server URI
    ///
    /// Supported formats:
    /// - `stdio://command` - Execute command and communicate via stdin/stdout
    /// - `tcp://host:port` - Connect to TCP socket
    /// - `unix:///path/to/socket` - Connect to Unix domain socket
    ///
    /// If not provided, falls back to `MCP_SERVER` environment variable.
    pub fn server_uri(mut self, uri: impl Into<String>) -> Self {
        self.server_uri = Some(uri.into());
        self
    }

    /// Set the authentication token
    ///
    /// If not provided, falls back to `MCP_TOKEN` environment variable.
    pub fn auth_token(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// Set the model name to report in responses
    ///
    /// This is the name that will appear in `ChatResponse.model` and `ModelInfo.name`.
    /// Defaults to "mcp-server" if not provided.
    pub fn model_name(mut self, name: impl Into<String>) -> Self {
        self.model_name = Some(name.into());
        self
    }

    /// Set the timeout for MCP operations
    ///
    /// Defaults to 30 seconds if not provided.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// Discover and configure from a named MCP server
    ///
    /// Searches for the server in MCP configuration files and automatically
    /// configures the provider from the discovered server config.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nxuskit_engine::prelude::*;
    /// # fn main() -> nxuskit_engine::Result<()> {
    /// // Discovers "filesystem" server from ~/.config/mcp/servers.json
    /// let provider = McpProvider::builder()
    ///     .discover_server("filesystem")
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn discover_server(mut self, server_name: impl Into<String>) -> Self {
        let name = server_name.into();

        // Try to discover servers
        if let Ok(servers) = McpProvider::discover_servers(None) {
            // Find the requested server
            if let Some(server) = servers.iter().find(|s| s.name == name) {
                // Configure from discovered server
                let server_uri = format!("stdio://{}", server.command);
                self.server_uri = Some(server_uri);
                self.model_name = Some(name);
            }
        }

        self
    }

    /// Build the MCP provider
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No server URI is provided and `MCP_SERVER` env var is not set
    /// - Server URI format is invalid
    pub fn build(self) -> Result<McpProvider> {
        // Get server URI from builder or environment
        let server_uri = self
            .server_uri
            .or_else(|| std::env::var("MCP_SERVER").ok())
            .ok_or_else(|| {
                NxuskitError::Configuration(
                    "MCP server URI not provided. Set via builder or MCP_SERVER env var"
                        .to_string(),
                )
            })?;

        // Validate URI format
        if server_uri.is_empty() {
            return Err(NxuskitError::Configuration(
                "MCP server URI cannot be empty".to_string(),
            ));
        }

        // Basic URI format validation
        if !server_uri.starts_with("stdio://")
            && !server_uri.starts_with("tcp://")
            && !server_uri.starts_with("unix://")
        {
            return Err(NxuskitError::Configuration(format!(
                "Invalid MCP server URI format: {}. Expected stdio://, tcp://, or unix://",
                server_uri
            )));
        }

        // Get auth token from builder or environment
        let auth_token = self.auth_token.or_else(|| std::env::var("MCP_TOKEN").ok());

        let model_name = self.model_name.unwrap_or_else(|| "mcp-server".to_string());
        let timeout = self.timeout.unwrap_or(DEFAULT_TIMEOUT);

        Ok(McpProvider {
            server_uri,
            auth_token,
            model_name,
            timeout,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex to serialize tests that modify MCP environment variables
    static MCP_ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_builder_missing_server_uri() {
        let _guard = MCP_ENV_MUTEX.lock().unwrap();

        // Ensure MCP_SERVER and MCP_TOKEN are not set
        unsafe {
            std::env::remove_var("MCP_SERVER");
            std::env::remove_var("MCP_TOKEN");
        }

        let result = McpProvider::builder().build();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            NxuskitError::Configuration(_)
        ));

        // Additional cleanup to prevent test interference
        unsafe {
            std::env::remove_var("MCP_SERVER");
            std::env::remove_var("MCP_TOKEN");
        }
    }

    #[test]
    fn test_builder_with_server_uri() {
        let provider = McpProvider::builder()
            .server_uri("stdio://mcp-server")
            .build()
            .unwrap();

        assert_eq!(provider.provider_name(), "mcp");
        assert_eq!(provider.server_uri, "stdio://mcp-server");
        assert_eq!(provider.model_name, "mcp-server");
    }

    #[test]
    fn test_builder_with_custom_model_name() {
        let provider = McpProvider::builder()
            .server_uri("stdio://test")
            .model_name("custom-model")
            .build()
            .unwrap();

        assert_eq!(provider.model_name, "custom-model");
    }

    #[test]
    fn test_builder_with_auth_token() {
        let provider = McpProvider::builder()
            .server_uri("tcp://localhost:3000")
            .auth_token("test-token")
            .build()
            .unwrap();

        assert_eq!(provider.auth_token, Some("test-token".to_string()));
    }

    #[test]
    fn test_builder_invalid_uri_format() {
        let result = McpProvider::builder().server_uri("invalid://uri").build();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NxuskitError::Configuration(_)));
        assert!(err.to_string().contains("Invalid MCP server URI format"));
    }

    #[test]
    fn test_builder_empty_uri() {
        let result = McpProvider::builder().server_uri("").build();

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            NxuskitError::Configuration(_)
        ));
    }

    #[test]
    fn test_builder_with_timeout() {
        let timeout = Duration::from_secs(60);
        let provider = McpProvider::builder()
            .server_uri("stdio://test")
            .timeout(timeout)
            .build()
            .unwrap();

        assert_eq!(provider.timeout, timeout);
    }

    #[test]
    fn test_debug_redacts_auth_token() {
        let provider = McpProvider::builder()
            .server_uri("stdio://test")
            .auth_token("secret-token")
            .build()
            .unwrap();

        let debug_str = format!("{:?}", provider);
        assert!(!debug_str.contains("secret-token"));
        assert!(debug_str.contains("[REDACTED]"));
    }

    #[test]
    fn test_builder_env_var_fallback() {
        let _guard = MCP_ENV_MUTEX.lock().unwrap();

        unsafe {
            std::env::set_var("MCP_SERVER", "stdio://env-server");
            std::env::set_var("MCP_TOKEN", "env-token");
        }

        let provider = McpProvider::builder().build().unwrap();

        assert_eq!(provider.server_uri, "stdio://env-server");
        assert_eq!(provider.auth_token, Some("env-token".to_string()));

        // Clean up
        unsafe {
            std::env::remove_var("MCP_SERVER");
            std::env::remove_var("MCP_TOKEN");
        }
    }

    #[tokio::test]
    async fn test_list_models() {
        let provider = McpProvider::builder()
            .server_uri("stdio://test")
            .model_name("test-model")
            .build()
            .unwrap();

        let models = provider.list_models().await.unwrap();

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "test-model");
        assert_eq!(models[0].metadata.get("provider"), Some(&"mcp".to_string()));
        assert_eq!(
            models[0].metadata.get("modalities"),
            Some(&"tools,resources".to_string())
        );
        assert_eq!(
            models[0].metadata.get("capabilities"),
            Some(&"tools,resources,prompts".to_string())
        );
        assert_eq!(
            models[0].metadata.get("server_uri"),
            Some(&"stdio://test".to_string())
        );
        assert!(models[0].description.is_some());
        assert!(
            models[0]
                .description
                .as_ref()
                .unwrap()
                .contains("tools, resources")
        );
    }

    #[tokio::test]
    async fn test_chat_returns_not_implemented_error() {
        use crate::Message;

        let provider = McpProvider::builder()
            .server_uri("stdio://test")
            .build()
            .unwrap();

        let request = ChatRequest::new("test").with_message(Message::user("Hello"));
        let result = provider.chat(&request).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NxuskitError::NotImplemented { .. }));
        let err_msg = err.to_string();
        assert!(err_msg.contains("not yet implemented"));
    }

    #[tokio::test]
    async fn test_list_tools_returns_not_implemented_error() {
        let provider = McpProvider::builder()
            .server_uri("stdio://test")
            .build()
            .unwrap();

        let result = provider.list_tools().await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NxuskitError::NotImplemented { .. }));
        assert!(err.to_string().contains("not yet implemented"));
    }

    #[tokio::test]
    async fn test_call_tool_returns_not_implemented_error() {
        let provider = McpProvider::builder()
            .server_uri("stdio://test")
            .build()
            .unwrap();

        let result = provider.call_tool("test_tool", None).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NxuskitError::NotImplemented { .. }));
        assert!(err.to_string().contains("not yet implemented"));
    }

    #[tokio::test]
    async fn test_list_resources_returns_not_implemented_error() {
        let provider = McpProvider::builder()
            .server_uri("stdio://test")
            .build()
            .unwrap();

        let result = provider.list_resources().await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NxuskitError::NotImplemented { .. }));
        assert!(err.to_string().contains("not yet implemented"));
    }

    #[tokio::test]
    async fn test_get_resource_returns_not_implemented_error() {
        let provider = McpProvider::builder()
            .server_uri("stdio://test")
            .build()
            .unwrap();

        let result = provider.get_resource("file://test").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NxuskitError::NotImplemented { .. }));
        assert!(err.to_string().contains("not yet implemented"));
    }

    #[test]
    fn test_valid_uri_formats() {
        let formats = vec![
            "stdio://mcp-server",
            "tcp://localhost:3000",
            "unix:///path/to/socket",
        ];

        for uri in formats {
            let result = McpProvider::builder().server_uri(uri).build();

            assert!(result.is_ok(), "URI {} should be valid", uri);
        }
    }

    // ============================================================================
    // TDD Tests for MCP Auto-Discovery (Phase 3)
    // ============================================================================

    mod discovery_tests {
        use super::*;
        use std::fs;
        use std::path::PathBuf;
        use std::sync::Mutex;
        use tempfile::TempDir;

        // Mutex to serialize discovery tests that modify NXUSKIT_MCP_CONFIG
        static DISCOVERY_ENV_MUTEX: Mutex<()> = Mutex::new(());

        /// Helper to create a test MCP config file
        fn create_test_config(dir: &TempDir, config_json: &str) -> PathBuf {
            let config_path = dir.path().join("servers.json");
            fs::write(&config_path, config_json).unwrap();
            config_path
        }

        #[test]
        fn test_discover_from_standard_location() {
            // Test: Should discover MCP servers from ~/.config/mcp/servers.json
            // This test verifies the standard config location works

            let temp_dir = TempDir::new().unwrap();
            let config_json = r#"{
                "servers": {
                    "filesystem": {
                        "command": "npx",
                        "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
                    },
                    "github": {
                        "command": "npx",
                        "args": ["-y", "@modelcontextprotocol/server-github"],
                        "env": {
                            "GITHUB_TOKEN": "test_token"
                        }
                    }
                }
            }"#;

            let config_path = create_test_config(&temp_dir, config_json);

            // This should discover servers from the config file
            let result = McpProvider::discover_servers(Some(config_path));
            assert!(result.is_ok(), "Discovery should succeed");

            let servers = result.unwrap();
            assert_eq!(servers.len(), 2, "Should discover 2 servers");

            // Verify filesystem server
            let fs_server = servers.iter().find(|s| s.name == "filesystem");
            assert!(fs_server.is_some(), "Should find filesystem server");
            let fs = fs_server.unwrap();
            assert_eq!(fs.command, "npx");
            assert_eq!(fs.args.len(), 3);

            // Verify github server
            let gh_server = servers.iter().find(|s| s.name == "github");
            assert!(gh_server.is_some(), "Should find github server");
            let gh = gh_server.unwrap();
            assert!(gh.env.contains_key("GITHUB_TOKEN"));
        }

        #[test]
        fn test_env_var_override() {
            let _guard = DISCOVERY_ENV_MUTEX.lock().unwrap();

            // Test: NXUSKIT_MCP_CONFIG should override standard location

            let temp_dir = TempDir::new().unwrap();
            let config_json = r#"{
                "servers": {
                    "custom": {
                        "command": "custom-server",
                        "args": ["--port", "3000"]
                    }
                }
            }"#;

            let config_path = create_test_config(&temp_dir, config_json);

            // Set environment variable
            unsafe {
                std::env::set_var("NXUSKIT_MCP_CONFIG", config_path.to_str().unwrap());
            }

            let result = McpProvider::discover_servers(None);
            assert!(result.is_ok(), "Discovery with env var should succeed");

            let servers = result.unwrap();
            assert_eq!(servers.len(), 1);
            assert_eq!(servers[0].name, "custom");

            // Cleanup
            unsafe {
                std::env::remove_var("NXUSKIT_MCP_CONFIG");
            }
        }

        #[test]
        fn test_invalid_json_returns_error() {
            // Test: Invalid JSON should return a descriptive error

            let temp_dir = TempDir::new().unwrap();
            let invalid_json = r#"{ "servers": { invalid json } }"#;
            let config_path = create_test_config(&temp_dir, invalid_json);

            let result = McpProvider::discover_servers(Some(config_path));
            assert!(result.is_err(), "Invalid JSON should fail");

            let err = result.unwrap_err();
            assert!(err.to_string().contains("Failed to parse MCP config"));
        }

        #[test]
        fn test_missing_config_returns_empty() {
            // Test: Missing config file should return empty list, not error

            let temp_dir = TempDir::new().unwrap();
            let nonexistent = temp_dir.path().join("nonexistent.json");

            let result = McpProvider::discover_servers(Some(nonexistent));
            assert!(result.is_ok(), "Missing config should not error");

            let servers = result.unwrap();
            assert_eq!(servers.len(), 0, "Should return empty list");
        }

        #[test]
        fn test_server_validation_command_required() {
            // Test: Server config must have a command field

            let temp_dir = TempDir::new().unwrap();
            let config_json = r#"{
                "servers": {
                    "invalid": {
                        "args": ["test"]
                    }
                }
            }"#;

            let config_path = create_test_config(&temp_dir, config_json);

            let result = McpProvider::discover_servers(Some(config_path));
            assert!(result.is_err(), "Missing command should fail validation");
        }

        #[test]
        fn test_server_with_optional_fields() {
            // Test: Server config with all optional fields

            let temp_dir = TempDir::new().unwrap();
            let config_json = r#"{
                "servers": {
                    "full": {
                        "command": "test-cmd",
                        "args": ["--verbose"],
                        "env": {
                            "API_KEY": "secret",
                            "DEBUG": "true"
                        }
                    }
                }
            }"#;

            let config_path = create_test_config(&temp_dir, config_json);

            let result = McpProvider::discover_servers(Some(config_path));
            assert!(result.is_ok());

            let servers = result.unwrap();
            assert_eq!(servers.len(), 1);

            let server = &servers[0];
            assert_eq!(server.command, "test-cmd");
            assert_eq!(server.args, vec!["--verbose"]);
            assert_eq!(server.env.len(), 2);
            assert_eq!(server.env.get("API_KEY"), Some(&"secret".to_string()));
        }

        #[test]
        fn test_create_provider_from_discovered_server() {
            // Test: Should be able to create McpProvider from discovered server

            let temp_dir = TempDir::new().unwrap();
            let config_json = r#"{
                "servers": {
                    "test": {
                        "command": "npx",
                        "args": ["-y", "@modelcontextprotocol/server-test"]
                    }
                }
            }"#;

            let config_path = create_test_config(&temp_dir, config_json);

            let servers = McpProvider::discover_servers(Some(config_path)).unwrap();
            assert_eq!(servers.len(), 1);

            // Should be able to create provider from discovered server
            let provider = McpProvider::from_server_config(&servers[0]);
            assert!(provider.is_ok());

            let provider = provider.unwrap();
            assert_eq!(provider.model_name, "test");
        }

        #[test]
        fn test_builder_with_discovery() {
            let _guard = DISCOVERY_ENV_MUTEX.lock().unwrap();

            // Test: Builder should support auto-discovery mode

            let temp_dir = TempDir::new().unwrap();
            let config_json = r#"{
                "servers": {
                    "auto": {
                        "command": "auto-server",
                        "args": []
                    }
                }
            }"#;

            let config_path = create_test_config(&temp_dir, config_json);
            unsafe {
                std::env::set_var("NXUSKIT_MCP_CONFIG", config_path.to_str().unwrap());
            }

            // Builder with discover() method should find servers
            let result = McpProvider::builder().discover_server("auto").build();

            assert!(result.is_ok(), "Discovery via builder should work");

            let provider = result.unwrap();
            assert_eq!(provider.model_name, "auto");

            // Cleanup
            unsafe {
                std::env::remove_var("NXUSKIT_MCP_CONFIG");
            }
        }
    }
}
