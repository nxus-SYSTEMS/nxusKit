# nxusKit MCP Server Integration Guide

## Overview

Model Context Protocol (MCP) servers extend nxusKit with external capabilities. Instead of being LLM providers themselves, MCP servers act as tool and resource providers that AI models can invoke to interact with external systems.

This guide covers the 4 recommended MCP servers that integrate seamlessly with nxusKit's current architecture.

## Quick Comparison

| Server | Purpose | Transport | Complexity | Best For |
|--------|---------|-----------|-----------|----------|
| **server-filesystem** | Local file access | stdio | Low | Reading/writing files, directory listing |
| **server-fetch** | Web content fetching | stdio | Low | Scraping websites, retrieving URLs |
| **mcp-shell-server** | Secure shell execution | stdio | Low | Running commands, system automation |
| **rust-mcp-filesystem** | High-performance files | stdio | Low | Native Rust integration, performance |

## Architecture

### How MCP Servers Work with nxusKit

```
┌──────────────┐
│ LLM Provider │  Chat request
│  (Claude)    │─────────────►┌─────────────────┐
└──────────────┘              │ nxusKit Agent  │
                              │  (with MCP)     │
                              └────────┬────────┘
                                       │
                    ┌──────────────────┼──────────────────┐
                    │                  │                  │
                    ▼                  ▼                  ▼
            ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
            │  File Tool   │  │  Web Fetch   │  │  Shell Tool  │
            │  (MCP Server)│  │  (MCP Server)│  │ (MCP Server) │
            └──────────────┘  └──────────────┘  └──────────────┘
```

Key points:
- MCP servers are **stateless, request-response** tools
- Each invocation is independent
- nxusKit acts as a **client** to MCP servers
- AI models decide which tools to use and when

## Getting Started

### Installation

All 4 servers use stdio transport (no special dependencies):

```bash
# Install Node-based servers (requires npm/node)
npm install -g @modelcontextprotocol/server-filesystem
npm install -g @modelcontextprotocol/server-fetch

# Install Python-based servers (requires python 3.8+)
pip install mcp-shell-server

# Or use uvx/npx to run without installation
export MCP_SERVER="stdio://npx @modelcontextprotocol/server-filesystem /path"
```

### Basic Usage Pattern

```rust
use nxuskit-engine::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create MCP provider
    let mcp = McpProvider::builder()
        .server_uri("stdio://npx @modelcontextprotocol/server-filesystem /home/user")
        .build()?;

    // List available tools from the MCP server
    let tools = mcp.list_tools().await?;
    println!("Available tools: {:?}", tools);

    // Use an LLM provider (Claude, OpenAI, etc.) alongside MCP
    let claude = ClaudeProvider::builder()
        .api_key(env::var("ANTHROPIC_API_KEY")?)
        .build()?;

    // Claude can now use MCP tools
    let request = ChatRequest::new("claude-opus")
        .with_message(Message::user("Read the file /home/user/config.json"));

    let response = claude.chat(&request).await?;
    println!("{}", response.content);

    Ok(())
}
```

## Server Selection Guide

### Use **server-filesystem** when you need to:
- Read files from the user's filesystem
- List directory contents
- Create/modify/delete files
- Implement file browsing features

**Example use cases**:
- Document processor: "Read all .md files in the docs folder"
- Code analyzer: "List Python files in src/ and check their sizes"
- Configuration manager: "Update the config.json file"

### Use **server-fetch** when you need to:
- Retrieve content from URLs
- Scrape website data
- Convert HTML to Markdown
- Fetch API responses

**Example use cases**:
- Research assistant: "Summarize the latest news from https://..."
- Documentation crawler: "Extract headers from the README"
- API integration: "Fetch the latest exchange rates from..."

### Use **mcp-shell-server** when you need to:
- Run system commands safely
- Execute build scripts
- Automate system tasks
- Integrate with CLI tools

**Example use cases**:
- Build automation: "Run tests and report failures"
- System monitoring: "Check disk usage and memory"
- Development workflow: "Compile the code and show errors"

**Security Note**: Requires explicit command whitelisting via `ALLOW_COMMANDS`

### Use **rust-mcp-filesystem** when you need to:
- Filesystem operations with maximum performance
- Native Rust integration
- No Node.js/Python dependencies
- Embedded MCP server (future use)

**Example use cases**:
- High-throughput file processing
- Performance-critical applications
- Rust-native deployments

## Common Workflows

### Workflow 1: Document Analysis
```
User: "Analyze the README.md file"
   ↓
Claude + server-fetch
   ├─ Fetch: Read URL from README
   └─ Summarize content
```

### Workflow 2: Codebase Exploration
```
User: "List all .rs files and show their sizes"
   ↓
Claude + server-filesystem
   ├─ List files in src/
   ├─ Get file sizes
   └─ Format results
```

### Workflow 3: Build & Test Automation
```
User: "Run tests and report any failures"
   ↓
Claude + mcp-shell-server
   ├─ Execute: cargo test
   ├─ Capture output
   └─ Parse and summarize errors
```

## Configuration

### Environment Variables

```bash
# Required: MCP server URI
export MCP_SERVER="stdio://npx @modelcontextprotocol/server-filesystem /home/user"

# Optional: Authentication token (if MCP server requires it)
export MCP_TOKEN="your-auth-token"
```

### CLI Usage

```bash
# List available models/tools from MCP server
cargo run -- models --provider mcp

# Send a message to the LLM with MCP tools available
export MCP_SERVER="stdio://npx @modelcontextprotocol/server-filesystem $HOME"
cargo run -- chat --provider claude --model claude-opus "Read my TODO list"
```

## Feature Matrix

| Feature | Filesystem | Fetch | Shell | Rust FS |
|---------|-----------|-------|-------|---------|
| Read files | ✓ | ✗ | ✗ | ✓ |
| Write files | ✓ | ✗ | ✗ | ✓ |
| List directory | ✓ | ✗ | ✗ | ✓ |
| Fetch URLs | ✗ | ✓ | ✗ | ✗ |
| Execute commands | ✗ | ✗ | ✓ | ✗ |
| Streaming responses | ✗ | ✓ | ✓ | ✗ |
| Authentication | ✗ | ✗ | ✗ | ✗ |
| Native Rust | ✗ | ✗ | ✗ | ✓ |

## Best Practices

### Security
1. **Always whitelist** commands in mcp-shell-server
2. **Validate** file paths in filesystem operations
3. **Limit scope** - only expose necessary directories
4. **Review logs** for unusual MCP server activity

### Performance
1. **Reuse** MCP provider instances
2. **Cache** file contents when appropriate
3. **Batch** multiple file operations
4. **Monitor** MCP server resource usage

### Reliability
1. **Handle errors** gracefully when MCP server unavailable
2. **Set timeouts** for long-running operations
3. **Test** MCP server connectivity on startup
4. **Log** MCP server activity for debugging

## Troubleshooting Guide

### "MCP server not found" / "command not found"

**Issue**: MCP server binary is not in PATH

**Solution**:
```bash
# Use full path or npx/uvx
export MCP_SERVER="stdio://npx @modelcontextprotocol/server-filesystem /path"

# Or install globally
npm install -g @modelcontextprotocol/server-filesystem
```

### "Permission denied" (filesystem operations)

**Issue**: MCP server doesn't have permission to access files

**Solution**:
```bash
# Ensure directory exists and is readable
ls -la /path/to/directory

# Run CLI with proper permissions
sudo cargo run -- ...
```

### "Command not allowed" (shell operations)

**Issue**: Command not in ALLOW_COMMANDS whitelist

**Solution**:
```bash
# Check whitelist
echo $ALLOW_COMMANDS

# Add missing commands
export ALLOW_COMMANDS="ls,pwd,find,grep,cat,echo,cargo"
```

### Performance issues

**Issue**: MCP operations are slow

**Solution**:
- Use `rust-mcp-filesystem` instead of Node.js version
- Check MCP server logs: `MCP_DEBUG=1`
- Verify network if using TCP transport
- Consider caching frequently accessed files

## Next Steps

1. **Read server-specific guides**:
   - [Filesystem Server Guide](MCP_FILESYSTEM.md)
   - [Fetch Server Guide](MCP_FETCH.md)
   - [Shell Server Guide](MCP_SHELL.md)
   - [Rust Filesystem Guide](MCP_RUST_FILESYSTEM.md)

2. **Run examples**:
   ```bash
   # Basic example
   export MCP_SERVER="stdio://npx @modelcontextprotocol/server-filesystem $HOME"
   cargo run --example mcp_example
   ```

3. **Integrate with your application**:
   - Combine MCP servers with LLM providers
   - Build multi-tool workflows
   - Extend with custom MCP servers (future)

## Resources

- [MCP Specification](https://spec.modelcontextprotocol.io/)
- [Official MCP Servers](https://github.com/modelcontextprotocol/servers)
- [Awesome MCP Servers](https://github.com/punkpeye/awesome-mcp-servers)
- [nxusKit Examples](https://github.com/nxus-SYSTEMS/nxusKit-examples)

## FAQ

**Q: Can I use multiple MCP servers at once?**
A: Not directly via nxusKit's current API. You would need to create a custom MCP server that aggregates tools from multiple servers, or orchestrate them at the application level.

**Q: Do MCP servers require internet?**
A: Only server-fetch requires internet. Filesystem and shell servers work offline.

**Q: Can I write my own MCP server?**
A: Yes! See the [MCP Specification](https://spec.modelcontextprotocol.io/) for protocol details. We plan to document custom MCP server development.

**Q: What's the performance overhead?**
A: Minimal for local operations. Network latency dominates for remote operations. Rust-based servers have ~5-10% overhead vs direct APIs.

**Q: Is MCP production-ready?**
A: Yes, the protocol and reference implementations are stable. Recommended servers are production-tested.
