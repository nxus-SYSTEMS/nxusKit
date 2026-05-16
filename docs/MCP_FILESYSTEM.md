# MCP Filesystem Server Integration Guide

## Overview

The `@modelcontextprotocol/server-filesystem` server provides safe, controlled file access to LLM applications. It enables AI models to read, list, and modify files within a designated directory.

**Official Repository**: https://github.com/modelcontextprotocol/servers

## Quick Start

### Launch the Server

```bash
# Option 1: Using npx (no installation needed)
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem /home/user/projects"

# Option 2: Using npm installation
npm install -g @modelcontextprotocol/server-filesystem
export MCP_SERVER="stdio://npx @modelcontextprotocol/server-filesystem /home/user/projects"

# Option 3: Specify multiple directories
export MCP_SERVER="stdio://npx @modelcontextprotocol/server-filesystem /home/user/projects /home/user/documents"
```

### Rust Example

```rust
use nxuskit-engine::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mcp = McpProvider::builder()
        .server_uri("stdio://npx -y @modelcontextprotocol/server-filesystem $HOME/projects")
        .build()?;

    // List available tools
    let tools = mcp.list_tools().await?;
    for tool in &tools {
        println!("Tool: {}", tool.name);
        println!("  Description: {}", tool.description);
    }

    Ok(())
}
```

### CLI Usage

```bash
# List available tools from the filesystem server
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem $HOME"
cargo run -- models --provider mcp

# Use with Claude to read a file
export ANTHROPIC_API_KEY="your-key"
cargo run -- chat --provider claude --model claude-opus \
  "Read the README.md file and summarize it"
```

## Available Tools

The filesystem server provides these tools to AI models:

### 1. **read_file**
Read the complete contents of a file.

**Input**:
```json
{
  "path": "/path/to/file.txt"
}
```

**Example Use Case**: "Read the configuration file and explain the settings"

### 2. **list_directory**
List contents of a directory with file metadata.

**Input**:
```json
{
  "path": "/path/to/directory"
}
```

**Output**: List of files with types (file/directory), sizes, and modified times

**Example Use Case**: "List all Python files in the src directory"

### 3. **write_file**
Write content to a file (creates or overwrites).

**Input**:
```json
{
  "path": "/path/to/file.txt",
  "content": "file contents"
}
```

**Example Use Case**: "Create a new configuration file with these settings"

### 4. **create_directory**
Create a new directory.

**Input**:
```json
{
  "path": "/path/to/new/directory"
}
```

**Example Use Case**: "Create a backup directory structure"

### 5. **move_file**
Rename or move a file/directory.

**Input**:
```json
{
  "source": "/old/path",
  "destination": "/new/path"
}
```

**Example Use Case**: "Organize files by renaming them with dates"

### 6. **delete_file**
Remove a file or empty directory.

**Input**:
```json
{
  "path": "/path/to/file"
}
```

**Example Use Case**: "Delete temporary files from the backup directory"

## Security Features

### Directory Sandboxing

Files are restricted to the specified root directory:

```bash
# This works - file is within /home/user/projects
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem /home/user/projects"
# AI can read /home/user/projects/config.json ✓

# This fails - path escapes via symlinks
# AI tries to read /etc/passwd ✗
```

### Path Validation

- Prevents directory traversal (`../` attacks)
- Validates symbolic links stay within sandbox
- Rejects absolute paths outside root directory
- Normalizes paths to prevent bypasses

### Recommended Practices

```bash
# ✓ Good: Specific project directory
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem $HOME/my-project"

# ⚠️ Caution: Home directory - ensure AI trustworthiness
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem $HOME"

# ❌ Never: Root directory - exposes system files
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem /"

# ❌ Never: Sensitive directories
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem $HOME/.ssh"
```

## Common Use Cases

### Use Case 1: Documentation Processor

```
User: "Update the API documentation with the new endpoint"

AI Model (using filesystem tools):
1. List files in docs/ → finds api.md
2. Read docs/api.md → understands current structure
3. Write docs/api.md → adds new endpoint documentation
```

**Example prompt**:
```
You are a documentation assistant. Use the filesystem tools to:
1. Read docs/api.md
2. Find the "Endpoints" section
3. Add documentation for the new /users/batch endpoint
4. Write the updated file back
```

### Use Case 2: Code Analysis

```
User: "Find all TODO comments in the codebase"

AI Model (using filesystem tools):
1. List files in src/ → gets all .rs files
2. Read each file → searches for TODO comments
3. Summarize findings → creates report
```

### Use Case 3: Configuration Management

```
User: "Update all environment-specific configs for production"

AI Model (using filesystem tools):
1. List config files in config/
2. Read current config
3. Modify settings for production
4. Write updated config
5. Create backup of old config
```

### Use Case 4: Project Scaffolding

```
User: "Create a new Node.js project structure"

AI Model (using filesystem tools):
1. Create directories: src/, tests/, docs/
2. Write package.json
3. Write src/index.js
4. Write .gitignore
5. Write README.md
```

## Practical Examples

### Example 1: Document Analysis Pipeline

```rust
use nxuskit-engine::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up filesystem MCP server
    let _mcp = McpProvider::builder()
        .server_uri("stdio://npx -y @modelcontextprotocol/server-filesystem $HOME/documents")
        .build()?;

    // Use Claude with filesystem access
    let claude = ClaudeProvider::builder()
        .api_key(std::env::var("ANTHROPIC_API_KEY")?)
        .build()?;

    let request = ChatRequest::new("claude-opus")
        .with_message(Message::user(
            "Using the filesystem tools, read the README.md file and provide: \
            1. A one-paragraph summary \
            2. List of key features \
            3. Setup instructions"
        ));

    let response = claude.chat(&request).await?;
    println!("Analysis:\n{}", response.content);

    Ok(())
}
```

### Example 2: Directory Structure Explorer

```rust
async fn explore_directory(
    mcp: &McpProvider,
    path: &str,
    depth: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    if depth > 3 {
        return Ok(());
    }

    // List directory contents
    println!("Directory: {}", path);
    // Would call list_directory tool via MCP

    Ok(())
}
```

### Example 3: Automated File Organization

```bash
#!/bin/bash

export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem $HOME/downloads"
export ANTHROPIC_API_KEY="your-key"

cargo run -- chat --provider claude --model claude-opus \
  "Organize the files in this directory by:
  1. Creating subdirectories for each file type (Documents, Images, Archives)
  2. Moving files into appropriate directories
  3. Reporting the organization structure created"
```

## Performance Characteristics

| Operation | Time | Notes |
|-----------|------|-------|
| Read small file (<1MB) | <10ms | Fast, memory efficient |
| List directory (100 files) | ~5ms | Very fast |
| Read large file (100MB+) | ~100ms | Limited by I/O |
| Write file | ~10ms | Depends on disk speed |
| Create directory | <1ms | Immediate |
| Move/rename | ~5ms | Disk-dependent |

**Tips for better performance**:
- Read files selectively (use grep/search where possible)
- Batch directory operations
- Avoid reading large binary files
- Cache file contents in application memory

## Configuration

### Environment Variables

```bash
# Required: Root directory to expose
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem /path/to/root"

# Optional: Multiple directories (space-separated)
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem /path1 /path2 /path3"

# Optional: Authentication (if custom server requires)
export MCP_TOKEN="your-token"
```

### Best Configuration Patterns

**Pattern 1: Single Project**
```bash
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem $HOME/my-project"
```
Use for: AI working on specific project

**Pattern 2: Multiple Projects**
```bash
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem $HOME/projects"
```
Use for: AI with access to multiple projects

**Pattern 3: Workspace**
```bash
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem $HOME/workspace"
```
Use for: Development environment with code and docs

## Error Handling

### Common Errors

**"Path not found"**
```
Error: File does not exist
→ Verify file path is within MCP_SERVER root directory
→ Check file spelling and case sensitivity
```

**"Permission denied"**
```
Error: Cannot read/write file
→ Check file permissions: ls -la /path/to/file
→ Ensure MCP server process has appropriate permissions
```

**"Path escapes sandbox"**
```
Error: Access denied - path outside allowed directory
→ All paths must be relative to root directory
→ Symlinks must point within root directory
```

### Graceful Error Handling

```rust
// Handle file not found gracefully
match mcp.call_tool("read_file", &params).await {
    Ok(content) => println!("File content: {}", content),
    Err(e) if e.to_string().contains("not found") => {
        println!("File doesn't exist, creating it...");
        // Create file with default content
    }
    Err(e) => return Err(e),
}
```

## Advanced Usage

### Dynamic File Discovery

Combine filesystem tools to discover and process files:

```
User: "Find all .rs files with TODO comments"

Steps:
1. List files in src/ (recursively)
2. Filter to .rs files
3. Read each .rs file
4. Search for TODO comments
5. Report findings
```

### Batch Operations

Perform multiple operations efficiently:

```
User: "Create a backup of all config files"

Steps:
1. Create backup/ directory
2. List config files
3. For each config file:
   - Read it
   - Write to backup/ with timestamp
4. Report backup status
```

### Version Control Integration

While not Git-specific, filesystem operations enable:
- Reading .gitignore to understand project structure
- Creating commit-like snapshots
- Implementing version management

## Testing

### Test Connectivity

```bash
# Verify MCP server starts correctly
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem $HOME"
cargo run -- models --provider mcp

# Should output available tools
```

### Integration Test

```bash
#!/bin/bash

# Create test file
echo "test content" > /tmp/test-file.txt

# Set up MCP server for /tmp
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem /tmp"

# Test file reading with Claude
cargo run -- chat --provider claude --model claude-opus \
  "Read test-file.txt and tell me its content"
```

## Troubleshooting

### "Module not found: @modelcontextprotocol/server-filesystem"

**Solution**:
```bash
# Install Node dependencies
npm install -g @modelcontextprotocol/server-filesystem

# Or use npx which auto-installs
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem /path"
```

### "EACCES: permission denied"

**Solution**:
```bash
# Check directory permissions
ls -ld /home/user/projects

# Fix permissions if needed
chmod 755 /home/user/projects
chmod 644 /home/user/projects/files
```

### "Path traversal blocked"

**Solution**:
```bash
# Use absolute paths within root directory
# ❌ Wrong: ../../../etc/passwd
# ✓ Right: /home/user/projects/config.json
```

## Security Considerations

### AI Trustworthiness

Be cautious exposing sensitive directories to untrusted AI models:
- **Safe**: Public project files, documentation
- **Caution**: Configuration files, private code
- **Dangerous**: SSH keys, credentials, system files

### Recommended Setup

```bash
# Create a dedicated directory for AI access
mkdir -p $HOME/ai-workspace
mkdir -p $HOME/ai-workspace/projects
mkdir -p $HOME/ai-workspace/documents

# Only expose this directory
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem $HOME/ai-workspace"

# Keep sensitive files elsewhere
# ~/.ssh, ~/.aws, /etc, etc. are not accessible
```

## Performance Optimization

### For Large Projects

```bash
# Avoid exposing entire home directory
# ❌ export MCP_SERVER="stdio://npx ... $HOME"

# Instead, expose specific project
# ✓ export MCP_SERVER="stdio://npx ... $HOME/specific-project"
```

### Caching Strategy

```rust
// Cache frequently accessed files
let mut file_cache: HashMap<String, String> = HashMap::new();

// Check cache before reading
if let Some(content) = file_cache.get(path) {
    use_content(content);
} else {
    // Read and cache
    let content = mcp.read_file(path).await?;
    file_cache.insert(path.to_string(), content);
}
```

## Next Steps

1. **Start simple**: Begin with read-only access to understand the model's behavior
2. **Add write access**: Gradually enable file modifications as you gain confidence
3. **Monitor operations**: Log and review what files the model accesses
4. **Combine tools**: Use alongside fetch server for web + local file integration
5. **Automate workflows**: Build repeatable document processing pipelines

## See Also

- [MCP Servers Overview](MCP_SERVERS_OVERVIEW.md)
- [Fetch Server Guide](MCP_FETCH.md)
- [Shell Server Guide](MCP_SHELL.md)
- [MCP Comparison & Best Practices](MCP_COMPARISON.md)
