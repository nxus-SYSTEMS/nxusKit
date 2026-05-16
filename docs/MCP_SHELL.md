# MCP Shell Server Integration Guide

## Overview

The `tumf/mcp-shell-server` provides secure, controlled shell command execution for LLM applications. Commands are whitelisted for safety, preventing arbitrary command execution.

**Repository**: https://github.com/tumf/mcp-shell-server
**Downloads**: 35.6K+ (as of Dec 2024)

## Quick Start

### Launch the Server

```bash
# Using uvx (recommended)
export MCP_SERVER="stdio://uvx mcp-shell-server"
export ALLOW_COMMANDS="ls,pwd,echo,cat,grep,find,wc"

# Or with pip
pip install mcp-shell-server
export MCP_SERVER="stdio://mcp-shell-server"
export ALLOW_COMMANDS="ls,pwd,echo,cat,grep,find,wc"
```

### Rust Example

```rust
use nxuskit-engine::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure shell server with command whitelist
    std::env::set_var("ALLOW_COMMANDS", "ls,pwd,find,cargo");

    let mcp = McpProvider::builder()
        .server_uri("stdio://uvx mcp-shell-server")
        .build()?;

    // Use with Claude for build automation
    let claude = ClaudeProvider::builder()
        .api_key(std::env::var("ANTHROPIC_API_KEY")?)
        .build()?;

    let request = ChatRequest::new("claude-opus")
        .with_message(Message::user("Run cargo test and report any failures"));

    let response = claude.chat(&request).await?;
    println!("Test Results:\n{}", response.content);

    Ok(())
}
```

### CLI Usage

```bash
export MCP_SERVER="stdio://uvx mcp-shell-server"
export ALLOW_COMMANDS="ls,pwd,find,echo"
export ANTHROPIC_API_KEY="your-key"

# Use Claude to explore directory structure
cargo run -- chat --provider claude --model claude-opus \
  "List all .rs files in the current directory and show their size"
```

## Security Features

### Command Whitelisting

Only explicitly allowed commands can be executed:

```bash
# Configure allowed commands
export ALLOW_COMMANDS="ls,pwd,echo,cargo,rustc"

# These will work:
ls -la          # ✓ Listed
echo hello      # ✓ Listed
cargo build     # ✓ Listed

# These will fail:
rm -rf /        # ✗ Not listed
bash            # ✗ Not listed
python script   # ✗ Not listed
```

### Shell Injection Prevention

Commands are NOT interpreted by a shell, preventing injection attacks:

```bash
# ✓ Safe: Direct execution
# User: "Run: ls -la /tmp"
→ Executes: ls -la /tmp

# ✓ Safe: No shell operators processed
# User: "Run: echo hello | wc"
→ Would fail - pipe (|) is literal argument, not processed

# ✗ Unsafe in traditional shells:
# User: "Run: echo secret && cat /etc/passwd"
→ Would only execute echo command part (safer)
```

### Timeout Protection

Long-running commands are terminated:

```bash
# Prevent infinite loops
export MCP_SHELL_TIMEOUT="30s"  # Default timeout

# Commands taking > 30s are killed
```

## Available Tools

### 1. **execute**
Run a whitelisted command with arguments.

**Input**:
```json
{
  "command": "cargo",
  "args": ["test", "--release"],
  "cwd": "/path/to/project"
}
```

**Output**:
```json
{
  "stdout": "Command output",
  "stderr": "Error output if any",
  "exit_code": 0
}
```

**Example**: Run tests in a Rust project

### 2. **list_commands**
List available commands from whitelist.

**Output**:
```json
{
  "commands": ["ls", "pwd", "find", "cargo"]
}
```

## Recommended Command Whitelist

### For Development

```bash
export ALLOW_COMMANDS="ls,pwd,echo,find,grep,cat,wc,head,tail,cargo,rustc"
```

**Use cases**: Source code inspection, build execution, test running

### For System Administration

```bash
export ALLOW_COMMANDS="ls,pwd,find,grep,cat,wc,lsof,df,ps,chmod"
```

**Use cases**: File management, system inspection, permission changes

### For Data Processing

```bash
export ALLOW_COMMANDS="ls,find,cat,wc,grep,awk,sed,sort,uniq"
```

**Use cases**: File analysis, text processing, log inspection

### Minimal (Most Secure)

```bash
export ALLOW_COMMANDS="ls,pwd,find"
```

**Use cases**: Read-only filesystem exploration

### Never Add

```bash
# ❌ Don't add these
rm,sudo,bash,sh,python,perl,ruby,node
```

These can be used to bypass security measures.

## Common Use Cases

### Use Case 1: Build & Test Automation

```
User: "Run all tests and report failures"

AI Steps:
1. Execute: cargo test --release
2. Parse output for failures
3. Extract failed test names
4. Create summary report
```

**Example**:
```bash
export ALLOW_COMMANDS="cargo,rustc,ls,pwd"
cargo run -- chat --provider claude --model claude-opus \
  "Run cargo test and create a summary of:
  1. Total tests run
  2. Passed/failed counts
  3. List of failed tests with error messages"
```

### Use Case 2: Source Code Exploration

```
User: "Find all TODO comments in Rust files"

AI Steps:
1. Execute: find src -name "*.rs" -type f
2. For each file, execute: grep -n "TODO" file.rs
3. Collect results
4. Create organized report
```

### Use Case 3: System Monitoring

```
User: "Check if disk usage is critical"

AI Steps:
1. Execute: df -h
2. Parse disk usage percentages
3. Identify critical filesystems
4. Recommend cleanup actions
```

### Use Case 4: Deployment Verification

```
User: "Verify the deployed application is running correctly"

AI Steps:
1. Execute: ps aux | grep app
2. Check process status
3. Verify port is listening
4. Test health endpoint
5. Report status
```

## Practical Examples

### Example 1: Test Suite Analyzer

```rust
use nxuskit-engine::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("ALLOW_COMMANDS", "cargo,find,grep");

    let _mcp = McpProvider::builder()
        .server_uri("stdio://uvx mcp-shell-server")
        .build()?;

    let claude = ClaudeProvider::builder()
        .api_key(std::env::var("ANTHROPIC_API_KEY")?)
        .build()?;

    let request = ChatRequest::new("claude-opus")
        .with_message(Message::user(
            "Using shell commands, determine:
            1. How many test files exist
            2. Total number of tests (search for #[test])
            3. Which modules have tests
            4. Estimated test coverage based on file count"
        ));

    let response = claude.chat(&request).await?;
    println!("Analysis:\n{}", response.content);

    Ok(())
}
```

### Example 2: Build Pipeline Automation

```bash
#!/bin/bash

export MCP_SERVER="stdio://uvx mcp-shell-server"
export ALLOW_COMMANDS="cargo,rustc,find,pwd,ls"
export ANTHROPIC_API_KEY="your-key"

# Automate release build process
cargo run -- chat --provider claude --model claude-opus \
  "Execute this release build pipeline:
  1. Run cargo build --release
  2. If successful, find the binary in target/release/
  3. Check its size
  4. Report success with binary location"
```

### Example 3: Code Quality Check

```bash
export ALLOW_COMMANDS="cargo,grep,find,wc,pwd"

cargo run -- chat --provider claude --model claude-opus \
  "Analyze code quality:
  1. Count total lines of code (*.rs files)
  2. Find all unwrap() calls and report count
  3. Find all TODO comments
  4. Create a quality report"
```

## Performance Characteristics

| Operation | Time | Notes |
|-----------|------|-------|
| Simple command (ls) | 50-100ms | Fast, I/O bound |
| Build project | 5-30s | Depends on project size |
| Test suite | 10-120s | Depends on test count |
| File search (find) | 100ms-2s | Depends on directory size |

**Tips**:
- Cache command results when output is predictable
- Use specific paths to limit search scope
- Run quick checks before expensive operations

## Error Handling

### Common Errors

**"Command not allowed"**
```
Error: Command not in ALLOW_COMMANDS whitelist
→ Check $ALLOW_COMMANDS environment variable
→ Add command if it's needed and trusted
```

**"Exit code 1" / "Command failed"**
```
Error: Command executed but returned error
→ Typical for test failures, missing files, etc.
→ Check stderr output for specific error
```

**"Timeout"**
```
Error: Command took too long to complete
→ Increase MCP_SHELL_TIMEOUT if available
→ Optimize the command to run faster
→ Break into smaller steps
```

### Graceful Error Handling

```rust
match mcp.call_tool("execute", &params).await {
    Ok(result) => {
        if result.exit_code == 0 {
            println!("Success: {}", result.stdout);
        } else {
            println!("Command failed: {}", result.stderr);
        }
    }
    Err(e) if e.to_string().contains("not allowed") => {
        eprintln!("Command not whitelisted");
        return Err(e);
    }
    Err(e) => return Err(e),
}
```

## Advanced Usage

### Chaining Commands

Since pipes don't work directly, chain via application logic:

```rust
// Step 1: Find files
let find_output = execute_command("find", vec!["src", "-name", "*.rs"])?;

// Step 2: For each file, grep for TODO
let files: Vec<&str> = find_output.lines().collect();
for file in files {
    let grep_output = execute_command("grep", vec!["TODO", file])?;
    process_todos(&grep_output);
}
```

### Conditional Execution

```rust
// Only run tests if build succeeds
let build_result = execute_command("cargo", vec!["build"])?;
if build_result.exit_code == 0 {
    let test_result = execute_command("cargo", vec!["test"])?;
    report_results(&test_result);
}
```

### Environment Variable Passing

```rust
// Pass environment variables to commands
std::env::set_var("RUST_BACKTRACE", "1");

// Command will inherit these
execute_command("cargo", vec!["test"])?;
```

## Security Best Practices

### Principle of Least Privilege

```bash
# ✓ Good: Only necessary commands
export ALLOW_COMMANDS="ls,find,grep"

# ⚠️ Caution: Too many commands
export ALLOW_COMMANDS="ls,find,grep,awk,sed,perl,python,node,bash"

# ❌ Never: Dangerous commands
export ALLOW_COMMANDS="rm,sudo,bash,sh"
```

### Input Validation

When accepting user input for command parameters:

```rust
// ⚠️ Danger: User controls command
let user_cmd = get_user_input();
execute_command(&user_cmd)?;  // Unsafe!

// ✓ Safe: Fixed command, user controls args
let pattern = sanitize_user_input(get_user_input());
execute_command("grep", vec![&pattern, "file.txt"])?;
```

### Audit Logging

```rust
// Log all command executions
fn execute_and_log(cmd: &str, args: &[&str]) -> Result<Output> {
    println!("AUDIT: Executing {} with args {:?}", cmd, args);
    let result = execute_command(cmd, args)?;
    println!("AUDIT: Exit code: {}", result.exit_code);
    Ok(result)
}
```

## Configuration

### Environment Variables

```bash
# Required: Command whitelist
export ALLOW_COMMANDS="ls,pwd,find,grep,cargo"

# Required: MCP server URI
export MCP_SERVER="stdio://uvx mcp-shell-server"

# Optional: Command timeout
export MCP_SHELL_TIMEOUT="30s"

# Optional: Working directory
export MCP_SHELL_CWD="/home/user/projects"

# Optional: Authentication
export MCP_TOKEN="your-token"
```

## Testing

### Test Connectivity

```bash
export MCP_SERVER="stdio://uvx mcp-shell-server"
export ALLOW_COMMANDS="ls,pwd"

# Should list available tools
cargo run -- models --provider mcp
```

### Integration Test

```bash
export ALLOW_COMMANDS="pwd,echo"

cargo run -- chat --provider claude --model claude-opus \
  "Execute the command: pwd and tell me the current directory"
```

## Troubleshooting

### "Module not found: mcp-shell-server"

**Solution**:
```bash
pip install mcp-shell-server

# Or use uvx to auto-install
export MCP_SERVER="stdio://uvx mcp-shell-server"
```

### "Command not found" when it should work

**Solution**:
```bash
# Verify command is in PATH
which ls
which cargo

# Verify it's in ALLOW_COMMANDS
echo $ALLOW_COMMANDS

# Add missing command
export ALLOW_COMMANDS="$ALLOW_COMMANDS,newcommand"
```

### "Permission denied"

**Solution**:
- Check file/directory permissions
- Run with appropriate user (may need sudo, but use carefully)
- Check if command needs special permissions

## Advanced Scenarios

### Build System Integration

```bash
export ALLOW_COMMANDS="cargo,rustc,ls,pwd,find"

# AI can manage the build process
# - Check if clean build needed
# - Run build
# - Analyze compiler warnings
# - Suggest fixes
```

### Continuous Monitoring

```bash
# Monitor application health
export ALLOW_COMMANDS="ps,lsof,df,netstat"

# AI can:
# - Check if processes are running
# - Monitor resource usage
# - Detect port conflicts
# - Alert on issues
```

### Development Workflow Automation

```bash
export ALLOW_COMMANDS="git,cargo,rustc,grep,find"

# AI can:
# - Check git status
# - Build changes
# - Run tests
# - Identify warnings
# - Suggest improvements
```

## Combining with Other MCP Servers

### Shell + Filesystem

```bash
export MCP_SERVER_SHELL="stdio://uvx mcp-shell-server"
export MCP_SERVER_FILES="stdio://npx @modelcontextprotocol/server-filesystem $HOME"
export ALLOW_COMMANDS="find,grep,cargo"

# AI can:
# - Find files with grep
# - Read files with filesystem server
# - Run commands with shell
```

### Shell + Fetch

```bash
export ALLOW_COMMANDS="curl,echo,grep"
# AI can fetch URLs via curl and process results
```

## Performance Optimization

### Parallel Command Execution

```rust
use futures::future::join_all;

let commands = vec![
    ("ls", vec!["src"]),
    ("pwd", vec![]),
    ("find", vec![".", "-type", "f"]),
];

let futures: Vec<_> = commands
    .iter()
    .map(|(cmd, args)| execute_command_async(cmd, args))
    .collect();

let results = join_all(futures).await;
```

### Caching Results

```rust
let mut cache: HashMap<String, String> = HashMap::new();

fn execute_cached(cmd: &str, args: &[&str]) -> Result<String> {
    let key = format!("{} {:?}", cmd, args);
    if let Some(result) = cache.get(&key) {
        return Ok(result.clone());
    }

    let result = execute_command(cmd, args)?;
    cache.insert(key, result.clone());
    Ok(result)
}
```

## Next Steps

1. **Start with read-only commands**: ls, find, grep, cat
2. **Add analysis commands**: wc, head, tail
3. **Add build commands**: cargo, rustc
4. **Implement caching** for repeated operations
5. **Monitor and audit** all command executions

## See Also

- [MCP Servers Overview](MCP_SERVERS_OVERVIEW.md)
- [Filesystem Server Guide](MCP_FILESYSTEM.md)
- [Fetch Server Guide](MCP_FETCH.md)
- [MCP Comparison & Best Practices](MCP_COMPARISON.md)
