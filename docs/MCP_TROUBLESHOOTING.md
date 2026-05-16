# MCP Servers: Troubleshooting & Performance Guide

## Troubleshooting Index

Jump to your issue:
- [Installation Problems](#installation-problems)
- [Connectivity Issues](#connectivity-issues)
- [Performance Problems](#performance-problems)
- [Security & Permission Errors](#security--permission-errors)
- [Server-Specific Issues](#server-specific-issues)
- [Diagnostic Tools](#diagnostic-tools)

## Installation Problems

### Problem: "npm: command not found"

**Symptom**:
```
error: npm: command not found
```

**Cause**: Node.js/npm not installed

**Solutions**:

Option 1: Install Node.js
```bash
# macOS
brew install node

# Ubuntu/Debian
sudo apt-get install nodejs npm

# Windows
# Download from https://nodejs.org/
```

Option 2: Use npx directly (auto-installs)
```bash
# npx automatically installs if needed
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem /path"
```

Option 3: Use uvx (alternative to npx)
```bash
# For shell-server, use uvx
export MCP_SERVER="stdio://uvx mcp-shell-server"
```

---

### Problem: "pip: command not found"

**Symptom**:
```
error: pip: command not found
```

**Cause**: Python not installed

**Solutions**:

```bash
# macOS
brew install python

# Ubuntu/Debian
sudo apt-get install python3 python3-pip

# Windows
# Download from https://python.org/

# Then install mcp-shell-server
pip install mcp-shell-server
```

---

### Problem: "Module not found" after installation

**Symptom**:
```
error: Module not found: @modelcontextprotocol/server-filesystem
```

**Cause**: Package installed in wrong location or npm cache issues

**Solutions**:

```bash
# Solution 1: Use global installation
npm install -g @modelcontextprotocol/server-filesystem

# Solution 2: Use npx (simplest)
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem /path"

# Solution 3: Clear npm cache
npm cache clean --force
npm install -g @modelcontextprotocol/server-filesystem

# Solution 4: Use full path to module
export MCP_SERVER="stdio://$(npm root -g)/@modelcontextprotocol/server-filesystem /path"
```

---

### Problem: "cargo install" fails for rust-mcp-filesystem

**Symptom**:
```
error: could not compile `rust-mcp-filesystem`
```

**Cause**: Missing Rust toolchain or compilation error

**Solutions**:

```bash
# Install Rust if not present
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Update Rust
rustup update

# Try installation again with verbose output
cargo install rust-mcp-filesystem -v

# Build from source with diagnostics
git clone https://github.com/rust-mcp-stack/rust-mcp-filesystem
cd rust-mcp-filesystem
cargo build --release -v
```

---

## Connectivity Issues

### Problem: "MCP_SERVER environment variable not set"

**Symptom**:
```
Error: MCP_SERVER environment variable not set
```

**Cause**: Environment variable not exported

**Solutions**:

```bash
# Check if variable is set
echo $MCP_SERVER

# Set variable in current shell
export MCP_SERVER="stdio://npx @modelcontextprotocol/server-filesystem /home/user"

# Verify it's set
echo $MCP_SERVER  # Should print the URI

# Make persistent (in ~/.bashrc or ~/.zshrc)
echo 'export MCP_SERVER="stdio://npx @modelcontextprotocol/server-filesystem /home/user"' >> ~/.bashrc
source ~/.bashrc
```

---

### Problem: "Connection refused" or "ECONNREFUSED"

**Symptom**:
```
Error: ECONNREFUSED - Connection refused
```

**Cause**: MCP server failed to start

**Solutions**:

```bash
# Check if server can start manually
stdio://npx @modelcontextprotocol/server-filesystem /tmp

# Verify path exists
ls -la /path/to/directory

# Check file permissions
ls -ld /path/to/directory
# Should show: drwxr-xr-x (755) or similar

# Test with simpler path first
export MCP_SERVER="stdio://npx @modelcontextprotocol/server-filesystem /tmp"
cargo run -- models --provider mcp

# Check for port conflicts (if applicable)
lsof -i :8080  # Check if port is in use
```

---

### Problem: "stdio pipe error" or "broken pipe"

**Symptom**:
```
Error: stdio pipe error: broken pipe
```

**Cause**: MCP server crashed or exited unexpectedly

**Solutions**:

```bash
# Enable debug logging
MCP_DEBUG=1 cargo run -- models --provider mcp

# Check server logs
export MCP_SERVER="stdio://npx @modelcontextprotocol/server-filesystem /path 2>&1"

# Try with explicit node path
export NODE_PATH="/usr/local/lib/node_modules"
export MCP_SERVER="stdio://npx @modelcontextprotocol/server-filesystem /path"

# Verify Node.js/Python versions
node --version  # Should be 14+
python3 --version  # Should be 3.8+

# Try with specific Node version
nvm use 18  # If using nvm
export MCP_SERVER="stdio://npx @modelcontextprotocol/server-filesystem /path"
```

---

## Performance Problems

### Problem: "Slow startup" (MCP initialization takes > 2 seconds)

**Symptom**:
```
Each operation starts slowly
Total time: 2-3 seconds for first operation
```

**Cause**: Node.js startup overhead

**Solutions**:

**Solution 1: Use Rust filesystem (10x faster)**
```bash
# Current (slow)
export MCP_SERVER="stdio://npx @modelcontextprotocol/server-filesystem /path"

# Improved (10x faster)
export MCP_SERVER="stdio://rust-mcp-filesystem /path"

# Result: 2-3s → 100-200ms
```

**Solution 2: Keep server running**
```bash
# Instead of restarting for each operation
# Keep one instance running and reuse it

# Start server once
rust-mcp-filesystem /path &
export MCP_SERVER="stdio://nc localhost 5000"
```

---

### Problem: "High memory usage"

**Symptom**:
```
MCP server using > 100MB RAM
System memory pressure increases
```

**Cause**: Inefficient operations or large file processing

**Solutions**:

```bash
# Monitor memory usage
ps aux | grep mcp-  # Check process memory

# Solution 1: Use Rust filesystem (5x less memory)
export MCP_SERVER="stdio://rust-mcp-filesystem /path"  # 5-10MB
# vs
export MCP_SERVER="stdio://npx @modelcontextprotocol/server-filesystem /path"  # 40-50MB

# Solution 2: Limit operations
# Instead of reading large files at once:
# ❌ read_file "large-100mb.bin"
# ✓ Use shell to read in chunks:
head -c 1024 large-file.bin

# Solution 3: Restart server periodically
# For long-running applications, restart MCP server
pkill -f mcp-shell-server
export MCP_SERVER="stdio://uvx mcp-shell-server"
```

---

### Problem: "Operations timing out"

**Symptom**:
```
Error: operation timed out after 30s
```

**Cause**: Slow network, large file, or blocked command

**Solutions**:

```bash
# For network operations (fetch)
export MCP_TIMEOUT="60000"  # 60 seconds

# For shell operations
export MCP_SHELL_TIMEOUT="60s"

# For filesystem
# Timeout is usually not needed for local ops

# Check what's slow
time cargo run -- models --provider mcp  # Time the operation

# Solution: Break into smaller operations
# ❌ Slow: Fetch huge file
# ✓ Fast: Fetch, then read in chunks
```

---

### Problem: "Slow file operations" (listing, reading)

**Symptom**:
```
List 1000 files takes > 100ms
Read 10MB file takes > 200ms
```

**Cause**: File I/O bottleneck or Node.js overhead

**Solutions**:

```bash
# Solution 1: Use Rust filesystem (2-4x faster)
export MCP_SERVER="stdio://rust-mcp-filesystem $LARGE_PROJECT"
# 100ms with Node.js → 30ms with Rust

# Solution 2: Optimize path scope
# ❌ Slow: Expose entire home directory
export MCP_SERVER="stdio://rust-mcp-filesystem $HOME"
# ✓ Fast: Specific project only
export MCP_SERVER="stdio://rust-mcp-filesystem $HOME/my-project"

# Solution 3: Cache results
# Store directory listings locally to avoid repeated scans

# Solution 4: Use SSD
# File I/O is much faster on SSD vs spinning disk
df /path  # Check disk type
```

---

### Performance Benchmark

Run benchmarks on your system:

```bash
#!/bin/bash

echo "=== MCP Server Performance Benchmark ==="

# Test 1: Startup time
echo "Test 1: Server startup time"
time cargo run -- models --provider mcp > /dev/null

# Test 2: File listing
echo "Test 2: List 100 files"
mkdir -p /tmp/test-mcp
for i in {1..100}; do touch /tmp/test-mcp/file-$i; done
time cargo run -- chat --provider claude --model claude-opus \
  "Using filesystem MCP server, list /tmp/test-mcp and count files"

# Test 3: File reading
echo "Test 3: Read 10MB file"
dd if=/dev/zero of=/tmp/test-mcp/large-file bs=1M count=10
time cargo run -- chat --provider claude --model claude-opus \
  "Read /tmp/test-mcp/large-file and report its size"

# Test 4: Multiple operations
echo "Test 4: 100 operations"
time for i in {1..100}; do
  cargo run -- models --provider mcp > /dev/null
done
```

---

## Security & Permission Errors

### Problem: "Permission denied" (file access)

**Symptom**:
```
Error: Permission denied (os error 13)
```

**Cause**: MCP server lacks read/write permissions

**Solutions**:

```bash
# Check current permissions
ls -la /path/to/file
ls -ld /path/to/directory

# Fix permissions
chmod 644 /path/to/file      # Make readable
chmod 755 /path/to/directory # Make directory accessible

# Check if running as correct user
whoami  # Current user
ls -l /path/to/file  # Who owns the file

# Run MCP server as file owner
sudo -u fileowner cargo run -- models --provider mcp

# Or change file ownership
sudo chown $USER:$USER /path/to/file
```

---

### Problem: "Path escapes sandbox"

**Symptom**:
```
Error: access denied - path outside allowed directory
```

**Cause**: Attempted directory traversal (security feature)

**Solutions**:

```bash
# ✓ Correct: Use paths within root
# Root: /home/user/projects
read_file: /home/user/projects/src/main.rs  ✓

# ❌ Blocked: Paths outside root
# Root: /home/user/projects
read_file: /etc/passwd  ✗ (outside root)
read_file: /../../../etc/passwd  ✗ (traversal attempt)

# Fix: Ensure all paths are relative to MCP root
export MCP_SERVER="stdio://rust-mcp-filesystem /home/user/projects"
# Now all paths are relative to /home/user/projects/
```

---

### Problem: "Command not allowed" (shell)

**Symptom**:
```
Error: Command 'gcc' not allowed
```

**Cause**: Command not in ALLOW_COMMANDS whitelist

**Solutions**:

```bash
# Check current whitelist
echo $ALLOW_COMMANDS

# Add missing command
export ALLOW_COMMANDS="$ALLOW_COMMANDS,gcc"

# Or set completely
export ALLOW_COMMANDS="ls,pwd,find,grep,gcc,make"

# For full list, list your commands
export ALLOW_COMMANDS="cargo,rustc,find,grep,echo,cat,pwd,ls"

# Verify it's set
echo $ALLOW_COMMANDS
```

---

## Server-Specific Issues

### Filesystem Server Issues

#### "Path not found"

```bash
# Verify the directory exists
test -d /path/to/directory && echo "exists" || echo "not found"

# Check from MCP root
export MCP_SERVER="stdio://rust-mcp-filesystem /home/user"
# Then access paths relative to /home/user/
# /home/user/documents/file.txt → access as: documents/file.txt

# Use absolute paths
ls /home/user/documents/file.txt
```

#### "Cannot write file"

```bash
# Check directory permissions
ls -ld /path/to/directory

# Ensure directory is writable
chmod u+w /path/to/directory

# Check disk space
df -h /path/to/directory  # Must have free space

# Verify file isn't locked
lsof /path/to/file  # Should be empty if not locked
```

---

### Fetch Server Issues

#### "Connection timeout"

```bash
# Check if URL is accessible from your network
curl -I https://example.com

# Increase timeout
export MCP_FETCH_TIMEOUT="60000"  # 60 seconds

# Check network connectivity
ping -c 3 8.8.8.8  # Google DNS

# Test DNS resolution
nslookup example.com
```

#### "SSL certificate error"

```bash
# Issue: Certificate validation failed
# Solution 1: Use URL with valid certificate
# ❌ export MCP_SERVER="stdio://npx @modelcontextprotocol/server-fetch"
# ✓ Use HTTPS URLs with valid certs

# Solution 2: For testing only (insecure)
export NODE_TLS_REJECT_UNAUTHORIZED=0  # Only for testing!

# Solution 3: Check certificate validity
openssl s_client -connect example.com:443
```

---

### Shell Server Issues

#### "Exit code 1" / "Command failed"

```bash
# This is normal for failed commands
# Check stderr for details

# Example: cargo test fails
# Exit code 1 is expected
# Read stderr to see which tests failed

# For build/test systems, parse the stderr for details
```

#### "Timeout"

```bash
# Increase timeout
export MCP_SHELL_TIMEOUT="120s"  # 2 minutes

# Check if command is actually slow
time cargo test --release

# Run less intensive version
cargo test --lib  # Faster than --all
```

---

### Rust Filesystem Issues

#### "Binary not found"

```bash
# Check if installed
which rust-mcp-filesystem
cargo install --list | grep rust-mcp-filesystem

# Install if missing
cargo install rust-mcp-filesystem

# Use full path if not in PATH
~/.cargo/bin/rust-mcp-filesystem /path
```

#### "Compilation fails"

```bash
# Update Rust
rustup update

# Check Rust version
rustc --version  # Should be recent

# Try installation with verbose output
cargo install rust-mcp-filesystem -v

# Check for disk space
df -h  # Must have space for compilation
```

---

## Diagnostic Tools

### Create Diagnostic Script

```bash
#!/bin/bash
# save as: diagnose-mcp.sh
# usage: ./diagnose-mcp.sh

echo "=== MCP Server Diagnostics ==="

# Check Node.js
echo -n "Node.js: "
node --version || echo "NOT FOUND"

# Check Python
echo -n "Python: "
python3 --version || echo "NOT FOUND"

# Check Rust
echo -n "Rust: "
rustc --version || echo "NOT FOUND"

# Check npm packages
echo -n "@modelcontextprotocol/server-filesystem: "
npm list -g @modelcontextprotocol/server-filesystem 2>/dev/null | grep -q "server-filesystem" && echo "INSTALLED" || echo "NOT FOUND"

# Check mcp-shell-server
echo -n "mcp-shell-server: "
pip show mcp-shell-server &>/dev/null && echo "INSTALLED" || echo "NOT FOUND"

# Check rust-mcp-filesystem
echo -n "rust-mcp-filesystem: "
cargo install --list | grep -q "rust-mcp-filesystem" && echo "INSTALLED" || echo "NOT FOUND"

# Check MCP_SERVER variable
echo -n "MCP_SERVER: "
[ -z "$MCP_SERVER" ] && echo "NOT SET" || echo "$MCP_SERVER"

# Test basic connectivity
echo ""
echo "Testing connectivity..."
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem /tmp"
cargo run -- models --provider mcp &>/dev/null && echo "✓ Connectivity OK" || echo "✗ Connectivity FAILED"
```

Run diagnostics:
```bash
chmod +x diagnose-mcp.sh
./diagnose-mcp.sh
```

---

### Debug Logging

```bash
# Enable debug output
export RUST_LOG=debug
export MCP_DEBUG=1

# Run with debugging
cargo run --example mcp_example

# Check output for warnings/errors
```

---

### Performance Profiling

```bash
#!/bin/bash
# Profile MCP server performance

echo "=== Performance Profile ==="

# Measure operation time
/usr/bin/time -v cargo run -- models --provider mcp

# Memory usage
ps aux | grep mcp

# CPU usage
top -p $(pgrep mcp-shell-server)  # If shell server is running

# Disk I/O
iostat -x 1 5  # Check disk busy percentage
```

---

## Getting Help

### When to Ask for Help

- Collected diagnostics using scripts above
- Noted exact error messages
- Tried all troubleshooting steps
- Identified which server has the issue

### Where to Ask

1. **Official Servers**: https://github.com/modelcontextprotocol/servers/issues
2. **Shell Server**: https://github.com/tumf/mcp-shell-server/issues
3. **Rust Filesystem**: https://github.com/rust-mcp-stack/rust-mcp-filesystem/issues
4. **nxusKit**: https://github.com/nxus-SYSTEMS/nxusKit/issues

### What to Include

1. Operating system and version
2. Output of `./diagnose-mcp.sh`
3. Error message (exact)
4. Steps to reproduce
5. Output with `MCP_DEBUG=1`

---

## See Also

- [MCP Servers Overview](MCP_SERVERS_OVERVIEW.md)
- [Comparison & Best Practices](MCP_COMPARISON.md)
- [Filesystem Guide](MCP_FILESYSTEM.md)
- [Fetch Guide](MCP_FETCH.md)
- [Shell Guide](MCP_SHELL.md)
- [Rust Filesystem Guide](MCP_RUST_FILESYSTEM.md)
