# MCP Example Workflows: Practical Patterns

This guide shows real-world workflows combining MCP servers with Claude LLM for intelligent automation.

## Quick Reference

| Workflow | MCP Servers | Purpose | File |
|----------|------------|---------|------|
| Document Analysis | Filesystem | Analyze local docs | `mcp_workflow_document_analysis.rs` |
| Web Research | Fetch | Research topics online | `mcp_workflow_web_research.rs` |
| Build Automation | Shell | CI/CD tasks | `mcp_workflow_build_automation.rs` |
| Multi-Tool | Filesystem + Fetch | Complex research | `mcp_workflow_multi_tool.rs` |

## Workflow 1: Document Analysis

**Purpose**: Analyze and improve project documentation
**MCP Server**: Filesystem
**Use Cases**:
- Documentation audit
- Quality assessment
- Organization review
- Content gap identification

### What It Does

```
1. List documentation files
2. Read and understand structure
3. Analyze with Claude
4. Identify gaps and issues
5. Generate improvement recommendations
```

### Setup

```bash
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem $HOME"
export ANTHROPIC_API_KEY="your-key"
cargo run --example mcp_workflow_document_analysis
```

### How to Adapt

**For different directory**:
```bash
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem /path/to/analyze"
```

**For different model**:
Edit the code, change `"claude-opus"` to another model:
```rust
let request = ChatRequest::new("claude-3-5-sonnet-20241022")
```

**For specific analysis**:
Modify the analysis prompts to focus on different aspects:
```rust
// Instead of general analysis
let request = ChatRequest::new("claude-opus")
    .with_message(Message::user(
        "Analyze only security-related documentation:
        1. Find all security.md files
        2. Check for authorization documentation
        3. Identify authentication guidance
        4. Find vulnerability reporting procedures"
    ));
```

### Example Output

```
=== MCP Workflow: Document Analysis ===

Analysis Results:
Found 15 markdown files organized in docs/ directory...

Detailed Review:
README.md is comprehensive but missing...

Recommendations:
Priority 1: Add API authentication guide
Priority 2: Create deployment documentation
Priority 3: Document configuration options
...
```

### Extending This Workflow

**Add code analysis**:
- Use Shell server to scan code comments
- Find // TODO comments and document them
- Track incomplete documentation

**Add external verification**:
- Use Fetch server to compare with official docs
- Verify examples still work
- Check for broken links

**Generate automated artifacts**:
- Create TOC automatically
- Generate API docs from code
- Build examples index

---

## Workflow 2: Web Research

**Purpose**: Research topics from multiple sources
**MCP Server**: Fetch
**Use Cases**:
- Competitive analysis
- Technology evaluation
- Trend research
- Information gathering

### What It Does

```
1. Fetch content from multiple URLs
2. Extract key information with Claude
3. Synthesize and compare findings
4. Generate structured report
```

### Setup

```bash
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-fetch"
export ANTHROPIC_API_KEY="your-key"
cargo run --example mcp_workflow_web_research
```

### How to Adapt

**For different topics**:
Edit the URLs and analysis in the Rust code:
```rust
let platforms = vec![
    ("AWS", "https://aws.amazon.com/"),
    ("DigitalOcean", "https://www.digitalocean.com/"),
    // Add your URLs here
];
```

**For pricing research**:
```rust
let request = ChatRequest::new("claude-opus")
    .with_message(Message::user(&format!(
        "Fetch and extract pricing from {}
        Provide:
        1. Pricing model (per-hour, per-month, etc.)
        2. Comparison to standard rates
        3. Special offers or discounts
        4. Enterprise pricing availability",
        url
    )));
```

**For feature comparison**:
```rust
let request = ChatRequest::new("claude-opus")
    .with_message(Message::user(&format!(
        "Research {} features related to {}
        List:
        1. Core features
        2. Advanced features
        3. Limitations
        4. Beta/experimental features",
        name, feature_category
    )));
```

### Example Output

```
Researching AWS (https://aws.amazon.com/)...
  ✓ Summarized

[Comparison Results showing features, pricing, use cases...]

Decision Framework:
Choose AWS if: Organization needs enterprise support
Choose Google Cloud if: You prioritize data analytics
Choose Azure if: You're already in Microsoft ecosystem
...
```

### Extending This Workflow

**Add structured data extraction**:
- Create a template for consistent extraction
- Build a comparison database
- Generate visualizations

**Add competitive monitoring**:
- Fetch latest updates regularly
- Track feature releases
- Monitor pricing changes

**Add sentiment analysis**:
- Fetch customer reviews
- Analyze pros/cons mentions
- Generate user satisfaction reports

---

## Workflow 3: Build Automation

**Purpose**: Intelligent CI/CD with decision-making
**MCP Server**: Shell
**Use Cases**:
- Build verification
- Test analysis
- Quality gates
- Progress reporting

### What It Does

```
1. Check project structure
2. Run build and tests
3. Analyze failures
4. Generate reports
5. Create improvement plan
```

### Setup

```bash
export MCP_SERVER="stdio://uvx mcp-shell-server"
export ALLOW_COMMANDS="cargo,rustc,find,grep,pwd,ls,echo"
export ANTHROPIC_API_KEY="your-key"
cargo run --example mcp_workflow_build_automation
```

### How to Adapt

**For different language** (Python):
```bash
export ALLOW_COMMANDS="python,pytest,find,grep,pip,ls,pwd"
```

**For different build system** (Node.js):
```bash
export ALLOW_COMMANDS="npm,node,jest,eslint,find,grep,ls"
```

**For additional checks**:
Edit the Rust code to add more commands:
```rust
let coverage_check = ChatRequest::new("claude-opus")
    .with_message(Message::user(
        "Check test coverage:
        1. Run: cargo tarpaulin (if installed)
        2. Extract coverage percentage
        3. Identify low-coverage modules
        4. Recommend coverage improvements"
    ));
```

### Security Considerations

**Command whitelist**:
```bash
# ✓ Good - minimal and necessary
export ALLOW_COMMANDS="cargo,rustc,find,grep"

# ⚠️ Dangerous - too many commands
export ALLOW_COMMANDS="cargo,python,node,bash,sh"
```

### Example Output

```
=== MCP Workflow: Build Automation ===

Project Analysis:
- Type: Binary application
- Dependencies: 25
- Modules: 8
- Maturity: Stable

Build Check Results:
- Compilation: ✓ Success
- Clippy: 3 warnings (style issues)
- Dependencies: All current

Test Results:
- Total tests: 47
- Passed: 46
- Failed: 1
- Coverage: 82%

Build Report:
Status: 🟡 Yellow
...
```

### Extending This Workflow

**Add performance profiling**:
- Benchmark build times
- Track regression
- Optimize slow steps

**Add security scanning**:
- Dependency vulnerability checks
- Code security analysis
- License compliance

**Add documentation validation**:
- Check for broken links
- Verify code examples compile
- Update generated docs

---

## Workflow 4: Multi-Tool Research (Advanced)

**Purpose**: Complex analysis combining multiple sources
**MCP Servers**: Filesystem + Fetch
**Use Cases**:
- Documentation audit with external comparison
- Architecture review
- Technology stack evaluation
- Security assessment

### What It Does

```
1. Analyze local resources (filesystem)
2. Research external resources (fetch)
3. Compare findings
4. Generate synthesis report
5. Create improvement plan
```

### Setup

```bash
export MCP_FILESYSTEM="stdio://npx -y @modelcontextprotocol/server-filesystem $HOME"
export MCP_FETCH="stdio://npx -y @modelcontextprotocol/server-fetch"
export ANTHROPIC_API_KEY="your-key"
cargo run --example mcp_workflow_multi_tool
```

### Advanced Pattern: Custom Tool Orchestration

```rust
// Combine multiple MCP servers in one workflow
async fn custom_workflow() -> Result<()> {
    // Step 1: Analyze local state with Filesystem
    let local_docs = fetch_local_docs()?;

    // Step 2: Research best practices with Fetch
    let best_practices = research_external()?;

    // Step 3: Have Claude analyze both
    let comparison = claude.analyze_both(local_docs, best_practices)?;

    // Step 4: Generate recommendations
    let plan = claude.create_improvement_plan(&comparison)?;

    Ok(())
}
```

### Creating Custom Workflows

To create your own workflow:

1. **Identify inputs**:
   - What data sources do you need?
   - Filesystem, web, APIs, commands?

2. **Plan Claude's role**:
   - Extraction and summarization
   - Analysis and comparison
   - Decision-making
   - Report generation

3. **Design prompts**:
   - Clear, specific instructions
   - Structured output format
   - Error handling expectations

4. **Implement pipeline**:
   ```rust
   let step1 = mcp_server_1.do_something();
   let step2 = mcp_server_2.do_something_else();
   let analysis = claude.analyze(step1, step2);
   let output = claude.generate_report(analysis);
   ```

### Example: Custom Documentation Audit

```rust
async fn documentation_audit() -> Result<()> {
    let claude = setup_claude()?;

    // Step 1: Local analysis
    let local = ChatRequest::new("claude-opus")
        .with_message(Message::user(
            "Analyze docs/ directory for documentation completeness"
        ));

    // Step 2: External comparison
    let external = ChatRequest::new("claude-opus")
        .with_message(Message::user(
            "Fetch https://best-practices.example.com \
             and compare documentation standards"
        ));

    // Step 3: Gap analysis
    let gaps = ChatRequest::new("claude-opus")
        .with_message(Message::user(&format!(
            "Compare local docs: {} \
             with best practices: {} \
             Identify gaps",
            local, external
        )));

    // Step 4: Report
    let report = claude.chat(&gaps).await?;
    println!("{}", report.content);

    Ok(())
}
```

---

## Common Patterns

### Pattern 1: Analyze → Report

```
MCP Server → Extract Data → Claude Analysis → Generate Report
```

**Example**: Analyze code → Find issues → Claude categorizes → Generate report

**Use cases**: Code reviews, security audits, performance analysis

### Pattern 2: Fetch → Research → Synthesize

```
External Source → Fetch Content → Claude Research → Comparison Report
```

**Example**: Fetch competing products → Extract features → Claude comparison

**Use cases**: Market research, technology evaluation

### Pattern 3: Multiple Sources → Merge → Decide

```
Source 1 ──┐
Source 2 ──┼─→ Claude Merge → Decision
Source 3 ──┘
```

**Example**: Local docs + web standards + best practices → unified plan

**Use cases**: Strategic planning, architecture decisions

### Pattern 4: Execute → Analyze → Improve

```
Command Execution → Output Analysis → Claude Recommendations → Improvement Plan
```

**Example**: Run tests → Parse output → Identify failures → Create fixes

**Use cases**: CI/CD, quality gates, automation

---

## Running All Examples

```bash
# Set up environment
export ANTHROPIC_API_KEY="your-key"

# Example 1: Document Analysis
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-filesystem $HOME"
cargo run --example mcp_workflow_document_analysis

# Example 2: Web Research
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-fetch"
cargo run --example mcp_workflow_web_research

# Example 3: Build Automation
export MCP_SERVER="stdio://uvx mcp-shell-server"
export ALLOW_COMMANDS="cargo,rustc,find,grep,pwd,ls,echo"
cargo run --example mcp_workflow_build_automation

# Example 4: Multi-Tool
export MCP_FILESYSTEM="stdio://npx -y @modelcontextprotocol/server-filesystem $HOME"
export MCP_FETCH="stdio://npx -y @modelcontextprotocol/server-fetch"
cargo run --example mcp_workflow_multi_tool
```

---

## Troubleshooting Workflows

### Workflow runs but produces generic output

**Issue**: Claude gives vague responses

**Solution**: Make prompts more specific
```rust
// ❌ Vague
"Analyze the code"

// ✓ Specific
"Analyze the code for:
1. Potential null pointer dereferences
2. Memory leaks
3. Thread safety issues
Format as: Issue | Location | Severity | Fix"
```

### MCP server fails to start

**Solution**: Check prerequisites
```bash
# Filesystem
npm list -g @modelcontextprotocol/server-filesystem

# Fetch
npm list -g @modelcontextprotocol/server-fetch

# Shell
pip show mcp-shell-server
```

### Commands timeout

**Solution**: Increase timeout or simplify workflow
```rust
let mut request = ChatRequest::new("claude-opus")
    .with_message(Message::user("Quick analysis..."));

// Run lighter analysis first
let quick = claude.chat(&request).await?;
```

---

## Building Your Own Workflow

### Step-by-Step Guide

1. **Define the goal**
   - What do you want to accomplish?
   - What information do you need?

2. **Choose MCP servers**
   - Filesystem: Local file access
   - Fetch: Web content
   - Shell: Command execution

3. **Design Claude's role**
   - Data extraction?
   - Analysis?
   - Decision-making?

4. **Write prompts**
   - Clear and specific
   - Structured output
   - Examples if helpful

5. **Implement in Rust**
   ```rust
   async fn my_workflow() -> Result<()> {
       let mcp = setup_mcp()?;
       let claude = setup_claude()?;

       // Your workflow steps here

       Ok(())
   }
   ```

6. **Test and iterate**
   - Run with test data
   - Refine prompts
   - Add error handling

---

## Next Steps

1. **Pick a workflow** that matches your use case
2. **Run the example** to understand the pattern
3. **Adapt the prompts** for your specific needs
4. **Create your own** workflow following the same pattern

See [MCP Documentation Index](MCP_DOCUMENTATION_INDEX.md) for more guides.
