# MCP Fetch Server Integration Guide

## Overview

The `@modelcontextprotocol/server-fetch` server enables LLM applications to retrieve and process web content. It fetches URLs, converts HTML to Markdown, and provides efficient content extraction.

**Official Repository**: https://github.com/modelcontextprotocol/servers

## Quick Start

### Launch the Server

```bash
# Option 1: Using npx (recommended)
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-fetch"

# Option 2: With npm installation
npm install -g @modelcontextprotocol/server-fetch
export MCP_SERVER="stdio://npx @modelcontextprotocol/server-fetch"
```

### Rust Example

```rust
use nxuskit-engine::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mcp = McpProvider::builder()
        .server_uri("stdio://npx -y @modelcontextprotocol/server-fetch")
        .build()?;

    // Use with Claude for web research
    let claude = ClaudeProvider::builder()
        .api_key(std::env::var("ANTHROPIC_API_KEY")?)
        .build()?;

    let request = ChatRequest::new("claude-opus")
        .with_message(Message::user(
            "Fetch and summarize the content from https://example.com"
        ));

    let response = claude.chat(&request).await?;
    println!("Summary:\n{}", response.content);

    Ok(())
}
```

### CLI Usage

```bash
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-fetch"
export ANTHROPIC_API_KEY="your-key"

# Use Claude to research a topic from web content
cargo run -- chat --provider claude --model claude-opus \
  "Fetch and summarize the latest Rust announcements from https://www.rust-lang.org"
```

## Available Tools

### 1. **fetch**
Retrieve and process content from a URL.

**Input**:
```json
{
  "url": "https://example.com/page"
}
```

**Output**:
```json
{
  "content": "Markdown-formatted content",
  "status_code": 200,
  "content_type": "text/html",
  "final_url": "https://actual-url.com/page"
}
```

**Features**:
- Automatic HTML → Markdown conversion
- Follows redirects
- Extracts main content (ignores headers/footers)
- Supports text, HTML, JSON content types
- Respects robots.txt and rate limits

### 2. **get_page_structure**
Get the structure of a page (headings, links) without full content.

**Input**:
```json
{
  "url": "https://example.com"
}
```

**Output**:
```json
{
  "title": "Page Title",
  "headings": ["H1 Title", "H2 Section"],
  "links": [{"text": "Link", "url": "https://..."}]
}
```

## Common Use Cases

### Use Case 1: Research Assistant

```
User: "Research the latest AI trends"

AI Steps:
1. Fetch https://techcrunch.com/ai
2. Extract key articles and trends
3. Summarize findings
4. Provide insights
```

**Example**:
```bash
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-fetch"
cargo run -- chat --provider claude --model claude-opus \
  "Summarize the top 5 AI news stories from https://news.ycombinator.com"
```

### Use Case 2: Documentation Extraction

```
User: "Extract API documentation from the official docs"

AI Steps:
1. Fetch https://docs.example.com/api
2. Extract endpoints and parameters
3. Format as organized documentation
4. Provide code examples
```

### Use Case 3: Content Aggregation

```
User: "Create a comparison of cloud pricing"

AI Steps:
1. Fetch AWS pricing page
2. Fetch GCP pricing page
3. Fetch Azure pricing page
4. Compare and summarize
```

### Use Case 4: Change Detection

```
User: "Monitor if the privacy policy changed"

AI Steps:
1. Fetch current https://example.com/privacy
2. Compare with previously fetched version
3. Report any significant changes
```

## Practical Examples

### Example 1: News Aggregation

```rust
use nxuskit-engine::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _mcp = McpProvider::builder()
        .server_uri("stdio://npx -y @modelcontextprotocol/server-fetch")
        .build()?;

    let claude = ClaudeProvider::builder()
        .api_key(std::env::var("ANTHROPIC_API_KEY")?)
        .build()?;

    let urls = vec![
        "https://news.ycombinator.com",
        "https://reddit.com/r/rust",
        "https://rust-lang.org/news",
    ];

    for url in urls {
        let request = ChatRequest::new("claude-opus")
            .with_message(Message::user(&format!(
                "Fetch and summarize the top 3 items from {}",
                url
            )));

        let response = claude.chat(&request).await?;
        println!("\n--- {} ---\n{}", url, response.content);
    }

    Ok(())
}
```

### Example 2: Documentation Crawler

```bash
#!/bin/bash

export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-fetch"
export ANTHROPIC_API_KEY="your-key"

# Extract API documentation
cargo run -- chat --provider claude --model claude-opus \
  "Fetch the API documentation from https://api.example.com/docs and:
  1. List all available endpoints
  2. Describe the authentication method
  3. Provide a quick-start code example
  4. List any rate limits"
```

### Example 3: Competitive Analysis

```bash
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-fetch"
cargo run -- chat --provider claude --model claude-opus \
  "Compare the pricing pages of:
  - https://competitor1.com/pricing
  - https://competitor2.com/pricing
  - https://our-product.com/pricing

  Create a comparison table with features and pricing"
```

## Supported Content Types

| Type | Support | Notes |
|------|---------|-------|
| HTML | ✓ | Converted to Markdown |
| Markdown | ✓ | Passed through |
| Plain text | ✓ | Preserved |
| JSON | ✓ | Formatted |
| PDF | ⚠️ | Limited support |
| Binary | ✗ | Not supported |

## Performance Characteristics

| Operation | Time | Notes |
|-----------|------|-------|
| Fetch small page (< 50KB) | 200-500ms | Network + processing |
| Fetch medium page (50-500KB) | 500ms-2s | Depends on network |
| Fetch large page (500KB+) | 2-5s | May timeout |
| Extract structure only | 100-300ms | Faster, less data |

**Tips**:
- For large pages, request page structure first to identify relevant sections
- Cache content locally when fetching the same URL multiple times
- Set appropriate timeouts in your application

## Security Considerations

### URL Validation

The fetch server validates URLs to prevent attacks:

```
✓ Safe URLs:
  - https://example.com
  - https://docs.example.com/api
  - https://static.example.com/file.json

✗ Blocked URLs:
  - file:///etc/passwd (file protocol)
  - http://localhost:8080 (private IPs - may be configurable)
  - http://192.168.1.1 (internal networks)
```

### Best Practices

```bash
# ✓ Good: Public websites only
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-fetch"

# ⚠️ Caution: Verify sources are trustworthy
# User provides URLs to fetch - validate them

# ❌ Never: Allow arbitrary internal URLs
# Don't expose internal APIs, staging servers, etc.
```

### Rate Limiting

Be aware of rate limits when fetching from public sites:

```rust
// Space out requests to avoid rate limiting
use std::time::Duration;
use tokio::time::sleep;

for url in urls {
    fetch_url(url).await?;
    sleep(Duration::from_millis(500)).await; // 500ms between requests
}
```

## Error Handling

### Common Errors

**"Connection timeout"**
```
Server took too long to respond
→ Try a different URL
→ Increase timeout in your application
→ Check if URL is accessible in your browser
```

**"Unsupported content type"**
```
Server returned binary or unsupported format
→ Try requesting a different content type
→ Use a different URL that returns HTML/text
```

**"Access denied"**
```
Server blocked the request (403/401)
→ Check if page requires authentication
→ Verify User-Agent isn't blocked
→ Try accessing through a public URL
```

### Graceful Error Handling

```rust
match mcp.call_tool("fetch", &params).await {
    Ok(content) => process_content(&content),
    Err(e) if e.to_string().contains("timeout") => {
        println!("Page took too long to load, using cached version");
        use_cache();
    }
    Err(e) if e.to_string().contains("not found") => {
        println!("URL returned 404");
        return Err(e);
    }
    Err(e) => return Err(e),
}
```

## Advanced Usage

### Batch Fetching with Caching

```rust
use std::collections::HashMap;

struct ContentCache {
    cache: HashMap<String, String>,
    ttl: std::time::Duration,
}

impl ContentCache {
    async fn fetch_or_cache(
        &mut self,
        url: &str,
        mcp: &McpProvider,
    ) -> Result<String> {
        if let Some(cached) = self.cache.get(url) {
            return Ok(cached.clone());
        }

        // Fetch from MCP
        let content = mcp.call_tool("fetch", url).await?;
        self.cache.insert(url.to_string(), content.clone());
        Ok(content)
    }
}
```

### Dynamic URL Discovery

```
User: "Extract links from the main page and fetch each one"

Steps:
1. Fetch https://example.com
2. Extract links (using get_page_structure or parsing)
3. For each link, fetch content
4. Aggregate and analyze
5. Report findings
```

### Multi-Step Research

```
User: "Research a topic across multiple sources"

Pipeline:
1. Search results → links
2. Fetch each link
3. Extract key information
4. Cross-reference findings
5. Create comprehensive report
```

## Testing

### Test Connectivity

```bash
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-fetch"

# Should list available tools
cargo run -- models --provider mcp
```

### Integration Test

```bash
# Test fetching from a reliable public URL
cargo run -- chat --provider claude --model claude-opus \
  "Fetch https://example.com and tell me what's on the page"
```

## Troubleshooting

### "Module not found"

**Solution**:
```bash
npm install -g @modelcontextprotocol/server-fetch
# Or use npx with -y flag to auto-install
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-fetch"
```

### "Connection refused" or "ECONNREFUSED"

**Solution**:
- Verify the URL is correct and the site is online
- Check your network connectivity
- Try the URL in your browser

### "Certificate error"

**Solution**:
```bash
# Set NODE_TLS_REJECT_UNAUTHORIZED=0 only for testing (insecure!)
# NODE_TLS_REJECT_UNAUTHORIZED=0 cargo run -- ...

# Better: Fix the actual certificate issue
# Or use a different URL that has valid certificates
```

### Page not fully loaded

**Issue**: Dynamic content not captured

**Solution**:
- Fetch server doesn't execute JavaScript
- Use static content URLs or APIs
- For SPAs, try fetching the API directly

## Configuration

### Environment Variables

```bash
# Required: Fetch server URI
export MCP_SERVER="stdio://npx -y @modelcontextprotocol/server-fetch"

# Optional: Proxy settings
export HTTP_PROXY="http://proxy.example.com:8080"
export HTTPS_PROXY="https://proxy.example.com:8443"

# Optional: Authentication
export MCP_TOKEN="your-token"
```

## Combining with Filesystem Server

Fetch and save content locally:

```bash
export MCP_SERVER_FETCH="stdio://npx @modelcontextprotocol/server-fetch"
export MCP_SERVER_FILES="stdio://npx @modelcontextprotocol/server-filesystem $HOME/downloads"

# User: "Fetch the README from https://github.com/example/project
#        and save it to my downloads folder"

# AI uses fetch to get content, filesystem to save it
```

## Performance Optimization

### Reduce Data Transfer

```
# Instead of fetching full page, get structure first
User: "Get links from https://example.com without loading the full page"

Steps:
1. Call get_page_structure()
2. Extract links
3. Only fetch relevant pages
```

### Parallel Fetching

```rust
// Fetch multiple URLs concurrently
use futures::future::join_all;

let futures: Vec<_> = urls
    .iter()
    .map(|url| mcp.call_tool("fetch", url))
    .collect();

let results = join_all(futures).await;
```

## Next Steps

1. **Start with simple URLs**: Test with reliable public websites first
2. **Build research workflows**: Create repeatable information gathering pipelines
3. **Combine with filesystem**: Save fetched content for local processing
4. **Add to workflows**: Integrate into multi-step research automation
5. **Monitor performance**: Track fetch times and adjust caching strategy

## See Also

- [MCP Servers Overview](MCP_SERVERS_OVERVIEW.md)
- [Filesystem Server Guide](MCP_FILESYSTEM.md)
- [Shell Server Guide](MCP_SHELL.md)
- [MCP Comparison & Best Practices](MCP_COMPARISON.md)
