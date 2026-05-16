# Troubleshooting Guide

This guide covers common issues and solutions for Rust, Go, and Python components of nxusKit.

---

## Rust Library Issues

### Compilation Errors

#### "error: could not compile `nxuskit-engine`"

**Causes:**
- Rust version too old
- Missing dependencies
- Syntax errors in code

**Solutions:**
```bash
# Check Rust version (should be 1.92+)
rustc --version

# Update Rust
rustup update

# Clean and rebuild
cargo clean
cargo build

# Check for dependency issues
cargo update
```

#### "error: cannot find crate `nxuskit-engine`"

**Causes:**
- Missing dependency in Cargo.toml
- Dependency not synced

**Solutions:**
```bash
# Add to Cargo.toml
[dependencies]
nxuskit-engine = "0.4.2"

# Sync dependencies
cargo build

# Or clean and rebuild
cargo clean
cargo build
```

#### "error: failed to run custom build command"

**Causes:**
- Missing build dependencies
- Compiler flags issue

**Solutions:**
```bash
# Check your C compiler is installed
cc --version

# For macOS
xcode-select --install

# For Linux
sudo apt-get install build-essential

# Try clean rebuild
cargo clean
cargo build --verbose
```

---

### Runtime Errors

#### "API key not found" or "OPENAI_API_KEY not set"

**Causes:**
- Environment variable not set
- Typo in variable name
- Running in different shell

**Solutions:**
```bash
# Set environment variable (temporary)
export OPENAI_API_KEY="sk-..."
cargo run

# Or for OpenAI
export ANTHROPIC_API_KEY="sk-ant-..."

# Check if set
echo $OPENAI_API_KEY

# Verify in code
println!("{:?}", env::var("OPENAI_API_KEY"));
```

**For Windows:**
```powershell
# PowerShell
$env:OPENAI_API_KEY = "sk-..."

# Command Prompt
set OPENAI_API_KEY=sk-...
```

#### "connection refused" or "Network error"

**For Ollama (local):**
```bash
# Check if Ollama is running
curl http://localhost:11434/api/tags

# Start Ollama
ollama serve

# Check with verbose
curl -v http://localhost:11434/api/tags

# Try alternate port
export OLLAMA_BASE_URL="http://localhost:11435"
```

**For Cloud APIs (OpenAI, Anthropic):**
```bash
# Check internet connection
ping google.com

# Check API endpoint
curl https://api.openai.com/v1/models -H "Authorization: Bearer $OPENAI_API_KEY"

# Check firewall
# May need to whitelist API domains

# Verify API key is valid
# Visit console.openai.com or console.anthropic.com
```

#### "Request timed out"

**Causes:**
- Model is slow
- Network latency
- Model overloaded

**Solutions:**
```rust
// Increase timeout in code (if supported by provider)
let request = ChatRequest::new("gpt-4o")
    .with_timeout(std::time::Duration::from_secs(120));

// For streaming, increase chunk read timeout
let mut stream = provider.chat_stream(&request).await?;
```

#### "Invalid model name"

**Causes:**
- Model doesn't exist
- Typo in model name
- Model not available for your account

**Solutions:**
```bash
# List available Ollama models
ollama list

# List available OpenAI models
curl https://api.openai.com/v1/models \
  -H "Authorization: Bearer $OPENAI_API_KEY" | jq '.data[].id'

# Use correct model name
let response = completion("gpt-4o", "prompt").await?;  // Correct
let response = completion("gpt-4", "prompt").await?;   // May work
let response = completion("gpt5", "prompt").await?;    // Will fail
```

---

### Testing Issues

#### "test failed: assertion error"

**Solutions:**
```bash
# Run test with output
cargo test -- --nocapture

# Run specific test
cargo test test_name -- --nocapture

# Run with more details
RUST_BACKTRACE=1 cargo test

# Run single-threaded to see order
cargo test -- --test-threads=1
```

#### "test timed out"

**Causes:**
- Test stuck in infinite loop
- Network call hanging
- Mock not set up correctly

**Solutions:**
```bash
# Set shorter timeout
cargo test -- --test-threads=1 --nocapture

# Check test code for blocking operations
// Bad - blocks
std::thread::sleep(Duration::from_secs(100));

// Good - async
tokio::time::sleep(Duration::from_secs(100)).await;

// Verify mock provider is being used
#[tokio::test]
async fn test_with_mock() {
    let provider = MockProvider::default();
    // ... test code
}
```

---

## Python Tools Issues

### Installation Issues

#### "ModuleNotFoundError: No module named 'requests'"

**Causes:**
- Dependencies not installed
- Wrong Python environment

**Solutions:**
```bash
# Install from requirements
pip install -r requirements.txt

# Install manually
pip install requests pytest Pillow

# Check Python version
python --version  # Should be 3.11+

# Check pip version
pip --version

# Verify installation
python -c "import requests; print(requests.__version__)"
```

#### "pip: command not found"

**Causes:**
- pip not in PATH
- Python not properly installed

**Solutions:**
```bash
# Use python -m pip
python -m pip install -r requirements.txt

# Or python3
python3 -m pip install -r requirements.txt

# On macOS, ensure Homebrew Python
brew install python
```

---

### Runtime Issues

#### "Connection refused" or "Unable to connect to Ollama"

**Causes:**
- Ollama not running
- Wrong host/port
- Firewall blocking

**Solutions:**
```bash
# Check if Ollama is running
curl http://localhost:11434/api/tags

# Start Ollama
ollama serve

# Try verbose
curl -v http://localhost:11434/api/tags

# Check default port
# Default: http://localhost:11434
# LM Studio: http://localhost:1234/v1

# If on different machine
export OLLAMA_BASE_URL="http://192.168.1.100:11434"

# Check firewall (macOS)
sudo lsof -i :11434
```

#### "Model not found" or "No such file or directory"

**Causes:**
- Model not pulled in Ollama
- Model is being downloaded
- Wrong model name

**Solutions:**
```bash
# List available models
ollama list

# Pull model
ollama pull llama2

# Wait for download to complete
# This can take 5-30 minutes depending on model size

# Check model name
ollama list | grep llama
# Use exact name: llama2, not llama or llama-2

# Pull specific version
ollama pull llama2:latest
ollama pull llama2:13b
```

#### "Test results files not created"

**Causes:**
- CSV/markdown flags not specified
- Script exited early
- File permission issue

**Solutions:**
```bash
# Must specify --csv or --markdown
python test_ollama_features.py --models llama2 --csv --markdown

# Check directory permissions
ls -la
# Should have write permissions (w)

# Try absolute path
python test_ollama_features.py --models llama2 \
  --csv /full/path/to/test_results.csv

# Check if script completed
echo $?  # Should be 0 (success)
```

#### "Import errors after update"

**Causes:**
- Cached Python files
- Dependency conflicts
- Version mismatch

**Solutions:**
```bash
# Clear Python cache
find . -type d -name __pycache__ -exec rm -r {} +
find . -name "*.pyc" -delete

# Reinstall dependencies
pip install --upgrade -r requirements.txt

# Or force reinstall
pip install --force-reinstall -r requirements.txt

# Check for conflicts
pip check
```

---

### Test Suite Issues

#### "Tests pass individually but fail when run together"

**Causes:**
- Shared state between tests
- Mock setup not isolated
- Fixture teardown issues

**Solutions:**
```bash
# Run tests individually
pytest tests/unit/test_csv_operations.py -v
pytest tests/unit/test_markdown_generation.py -v

# Run with verbose to see order
pytest tests/ -v -s

# Check fixtures are isolated
# Each test should get fresh mock/fixture

# Verify conftest.py
# Check pytest configuration
cat pytest.ini
```

#### "Coverage not increasing"

**Causes:**
- New code not being tested
- Tests not actually testing code
- Coverage tool configuration

**Solutions:**
```bash
# Check coverage report
pytest tests/ --cov=test_ollama_features --cov-report=term-missing

# See which lines aren't covered
pytest tests/ --cov=test_ollama_features --cov-report=html
# Open htmlcov/index.html in browser

# Write tests for uncovered lines
# Look at missing lines in report

# Verify tests actually run the code
# Add print() statements to debug
```

---

## Common Issues Across Both

### "Permission denied" errors

**Causes:**
- File/directory permissions
- Running as wrong user
- Read-only file system

**Solutions:**
```bash
# Check permissions
ls -la file_name
# Should show -rw- or -rwx

# Make executable if needed
chmod +x test_ollama_features.py

# Make writable if needed
chmod u+w file_name

# Check directory
ls -la ./
# Should be drwx... with your user

# Check disk space
df -h
# Should have free space
```

### "Module not found" / "Import errors"

**Causes:**
- Wrong working directory
- Python path not set
- Module not installed

**Solutions:**
```bash
# Check current directory
pwd

# Add to Python path
export PYTHONPATH="${PYTHONPATH}:/path/to/project"

# Or run from correct directory
cd nxusKit-tools/test-ollama/
python test_ollama_features.py --models llama2

# Verify module exists
python -c "import test_ollama_features; print('OK')"
```

### "UTF-8 encoding errors"

**Causes:**
- Terminal encoding not UTF-8
- Emoji in output
- File encoding issues

**Solutions:**
```bash
# Check terminal encoding
locale
# Should show UTF-8

# Set UTF-8 explicitly
export LANG=en_US.UTF-8
export LC_ALL=en_US.UTF-8

# Python UTF-8 mode
export PYTHONIOENCODING=utf-8

# In code (Python)
import sys
sys.stdout.reconfigure(encoding='utf-8')
```

---

## Debugging Tips

### Rust Debugging

```bash
# Detailed error messages
RUST_BACKTRACE=full cargo test

# Verbose output
cargo build --verbose

# Debug specific issue
cargo test test_name -- --nocapture --test-threads=1

# Check compiler warnings
cargo clippy
```

### Python Debugging

```bash
# Add debug prints
print(f"Debug: {variable}")

# Use Python debugger
import pdb; pdb.set_trace()

# Run with verbose
python -v test_ollama_features.py

# Check Python path
python -c "import sys; print(sys.path)"

# Trace imports
python -X importtime test_ollama_features.py
```

### General Debugging

```bash
# Check environment variables
env | grep -E "OPENAI|ANTHROPIC|OLLAMA"

# Network diagnostics
ping api.openai.com
curl -v https://api.openai.com/

# Check listening ports
netstat -tuln | grep 11434

# Check process running
ps aux | grep ollama
```

---

## Getting Help

1. **Check existing issues**: https://github.com/nxus-SYSTEMS/nxusKit/issues
2. **Search documentation**: [README.md](../README.md), [ARCHITECTURE.md](../ARCHITECTURE.md)
3. **Read guides**: [GETTING_STARTED.md](../GETTING_STARTED.md)
4. **File an issue** with:
   - Error message
   - Minimal reproduction steps
   - Your environment (Rust version, Python version, OS)
   - What you tried to fix it

---

## Still Stuck?

- **Rust issues**: Post in [GitHub Issues](https://github.com/nxus-SYSTEMS/nxusKit/issues/new?template=bug_report.md)
- **Go issues**: Same, mention it's Go-related
- **Python issues**: Same, mention it's Python-related
- **General questions**: [GitHub Discussions](https://github.com/nxus-SYSTEMS/nxusKit/discussions)

**We're here to help!** 🤝
