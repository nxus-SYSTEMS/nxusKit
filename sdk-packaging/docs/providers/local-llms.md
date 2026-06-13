# Local LLM Providers

nxusKit supports two categories of local LLM providers:

- **In-Process Providers** — Load and run models directly in your application process. No external server required. Requires Cargo feature flags.
- **HTTP-Based Providers** — Connect to a locally running inference server (Ollama, LM Studio). No feature flags needed.

---

## In-Process Providers

In-process providers load GGUF model files directly into your application and run inference without any external server. This gives you the lowest possible latency, full control over model lifecycle, and zero network overhead.

### Backends

nxusKit provides two in-process inference backends:

| Backend | Feature Flag | Engine | Status |
|---------|-------------|--------|--------|
| llama.cpp | `provider-local-llama` | [llama-cpp-2](https://github.com/utilityai/llama-cpp-rs) (safe Rust bindings to llama.cpp) | Mature upstream backend |
| mistral.rs | `provider-local-mistralrs` | [mistral.rs](https://github.com/EricLBuehler/mistral.rs) (pure-Rust inference on Candle) | Experimental |

Both backends load the same **GGUF** model format. If both features are enabled, you can select a backend explicitly or let nxusKit auto-select the first available one.

### Supported Model Format

**GGUF** (GPT-Generated Unified Format) is the only supported format. GGUF files are self-contained — they bundle weights, tokenizer, and metadata in a single file. Both backends read the same `.gguf` files.

You can obtain GGUF models from:

- [Hugging Face](https://huggingface.co/models?library=gguf) — Search for "GGUF" in model filters
- [Ollama](https://ollama.com/library) — Models pulled by Ollama are stored as GGUF blobs (nxusKit can discover these)
- [TheBloke on Hugging Face](https://huggingface.co/TheBloke) — Prolific quantizer of popular models

### Compatible Model Families

Any model published in GGUF format that is compatible with llama.cpp should work. The following model families are known to work:

| Model Family | Parameters | Example GGUF | Notes |
|---|---|---|---|
| **Llama 3.2** | 1B, 3B | `Llama-3.2-1B-Instruct-Q4_K_M.gguf` | Meta's latest small models. Great for CPU. |
| **Llama 3.1** | 8B, 70B, 405B | `Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf` | Excellent general-purpose. 8B runs well on 16GB RAM. |
| **Llama 3** | 8B, 70B | `Meta-Llama-3-8B-Instruct-Q4_K_M.gguf` | Predecessor to 3.1, widely available. |
| **Llama 2** | 7B, 13B, 70B | `llama-2-7b-chat.Q4_K_M.gguf` | Older and widely available. |
| **Mistral** | 7B | `mistral-7b-instruct-v0.3.Q4_K_M.gguf` | Strong performance relative to size. |
| **Mixtral** | 8x7B, 8x22B | `mixtral-8x7b-instruct-v0.1.Q4_K_M.gguf` | Mixture-of-experts. Needs more RAM. |
| **Phi-3 / Phi-3.5** | 3.8B, 14B | `Phi-3.5-mini-instruct-Q4_K_M.gguf` | Microsoft. Strong reasoning for size. |
| **Gemma 2** | 2B, 9B, 27B | `gemma-2-9b-it-Q4_K_M.gguf` | Google. Good coding and instruction following. |
| **Qwen 2.5** | 0.5B–72B | `Qwen2.5-7B-Instruct-Q4_K_M.gguf` | Alibaba. Multilingual, strong at math/code. |
| **DeepSeek-R1** | 1.5B–70B (distilled) | `DeepSeek-R1-Distill-Llama-8B-Q4_K_M.gguf` | Reasoning-focused distilled models. |
| **CodeLlama** | 7B, 13B, 34B | `codellama-7b-instruct.Q4_K_M.gguf` | Code-specialized Llama variant. |
| **TinyLlama** | 1.1B | `tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf` | Tiny. Useful for testing and CI. |
| **StableLM** | 1.6B, 3B | `stablelm-2-1_6b-chat.Q4_K_M.gguf` | Stability AI. Small and fast. |
| **Yi** | 6B, 9B, 34B | `Yi-1.5-9B-Chat-Q4_K_M.gguf` | 01.AI. Strong multilingual. |
| **Command R** | 35B, 104B | `c4ai-command-r-v01-Q4_K_M.gguf` | Cohere. RAG-optimized. |
| **Falcon** | 7B, 40B, 180B | `falcon-7b-instruct-Q4_K_M.gguf` | TII. |

> **Tip:** For testing and development, start with **TinyLlama 1.1B** (fast, tiny, runs anywhere) or **Llama 3.2 1B** (modern, instruction-tuned, still small enough for CPU).

### Quantization Levels

GGUF models come in different quantization levels that trade quality for size/speed. nxusKit auto-detects the quantization from the filename:

| Quantization | Bits/Weight | Relative Quality | Relative Size | Recommended For |
|---|---|---|---|---|
| `F32` | 32 | Best (reference) | Largest | Accuracy testing only |
| `F16` | 16 | Excellent | ~50% of F32 | GPU with sufficient VRAM |
| `Q8_0` | 8.5 | Near-lossless | ~50% of F16 | Quality-sensitive tasks |
| `Q6_K` | 6.6 | Very good | ~38% of F16 | Good quality/size balance |
| `Q5_K_M` | 5.7 | Good | ~33% of F16 | Balanced |
| `Q5_K_S` | 5.5 | Good | ~32% of F16 | Slightly smaller than K_M |
| `Q4_K_M` | 4.8 | Acceptable | ~27% of F16 | **Most popular**. Best quality/size tradeoff. |
| `Q4_K_S` | 4.5 | Acceptable | ~26% of F16 | Slightly smaller than K_M |
| `Q3_K_M` | 3.9 | Degraded | ~22% of F16 | Memory-constrained environments |
| `Q2_K` | 3.4 | Poor | ~18% of F16 | Extreme memory constraints only |

> **Recommendation:** Use **Q4_K_M** for most deployments. It provides the best balance of quality, speed, and memory usage.

### Memory Requirements

Approximate RAM needed to load a model (varies by quantization):

| Model Size | Q4_K_M | Q8_0 | F16 |
|---|---|---|---|
| 1B params | ~0.8 GB | ~1.2 GB | ~2 GB |
| 3B params | ~2 GB | ~3.5 GB | ~6 GB |
| 7-8B params | ~4.5 GB | ~8 GB | ~15 GB |
| 13B params | ~8 GB | ~14 GB | ~26 GB |
| 34B params | ~20 GB | ~36 GB | ~68 GB |
| 70B params | ~40 GB | ~72 GB | ~140 GB |

These are approximate. Actual usage depends on context window size, batch size, and KV cache.

### GPU Acceleration

GPU offloading is automatic when available:

| Platform | Acceleration | How to Enable |
|---|---|---|
| macOS (Apple Silicon) | Metal | Automatic — llama.cpp detects Metal at build time |
| Linux (NVIDIA) | CUDA | Install CUDA toolkit; llama-cpp-2 detects at build time |
| Linux (AMD) | ROCm/Vulkan | Vulkan support via llama.cpp build flags |
| Windows (NVIDIA) | CUDA | Install CUDA toolkit |
| All platforms | CPU (AVX2/NEON) | Always available as fallback |

Control GPU offloading with `n_gpu_layers`:
- `-1` — offload all layers to GPU (maximum acceleration)
- `0` — CPU only (default)
- `N` — offload first N layers (partial offloading for limited VRAM)

### Configuration

```json
{
  "provider_type": "local",
  "model": "/path/to/model.gguf",
  "options": {
    "backend": "llama-cpp",
    "n_gpu_layers": -1,
    "context_size": 4096,
    "batch_size": 512,
    "threads": 8
  }
}
```

**Configuration options:**

| Option | Type | Default | Description |
|---|---|---|---|
| `backend` | string | auto-detect | `"llama-cpp"` or `"mistralrs"`. Auto-selects first available if omitted. |
| `n_gpu_layers` | integer | `0` | GPU layer offloading. `-1` = all, `0` = CPU only, `N` = first N layers. |
| `context_size` | integer | model default | Context window size in tokens. |
| `batch_size` | integer | backend default | Prompt processing batch size. Higher = faster prompt processing, more memory. |
| `threads` | integer | auto-detect | CPU thread count. Leave unset for optimal auto-detection. |

### Rust API

```rust
use nxuskit-engine::{LocalRuntimeProvider, ChatRequest, Message};

let provider = LocalRuntimeProvider::builder()
    .model_path("/models/Llama-3.2-1B-Instruct-Q4_K_M.gguf")
    .n_gpu_layers(-1)      // Use GPU
    .context_size(4096)
    .build()?;

let request = ChatRequest::new("Llama-3.2-1B-Instruct-Q4_K_M.gguf")
    .with_message(Message::user("Explain quantum computing in one paragraph."))
    .with_temperature(0.7)
    .with_max_tokens(256);

let response = provider.chat(&request).await?;
println!("{}", response.content);
```

### Model Discovery

The provider can discover available models from multiple sources:

```rust
let models = provider.list_models().await?;
for model in &models {
    println!("{}: {} ({})",
        model.id,
        model.name,
        model.metadata.get("quantization").unwrap_or(&"unknown".into()));
}
```

**Discovery sources** (in priority order):

1. **Explicit path** — The model path in your configuration
2. **Search paths** — Directories you configure (scans for `.gguf` files)
3. **Ollama store** (opt-in) — Discovers models already pulled by Ollama

**Ollama store locations** (auto-detected):
- macOS: `~/.ollama/models`
- Linux: `/usr/share/ollama/.ollama/models` or `~/.ollama/models`
- Windows: `%USERPROFILE%\.ollama\models`

**Environment variables:**
- `NXUSKIT_MODELS` — Custom model search directory
- `OLLAMA_MODELS` — Override Ollama store location

### Model Cache Management

Models stay loaded in memory after first use. You can manage the cache explicitly:

```rust
// Pre-load a model (async, happens in background)
provider.preload_model("/models/llama-3.2-1b.Q4_K_M.gguf").await?;

// Check what's loaded
for info in provider.cached_models() {
    println!("{}: {} bytes", info.path, info.memory_bytes.unwrap_or(0));
}

// Free memory
provider.unload_model("/models/llama-3.2-1b.Q4_K_M.gguf");
```

### Capabilities

| Capability | Supported |
|---|---|
| System messages | Yes |
| Streaming | Yes (token-by-token) |
| Vision/images | No |
| JSON mode | No (llama-cpp) / Yes (mistral.rs) |
| Seed (deterministic) | Yes |
| Stop sequences | Yes (up to 4) |
| Temperature | Yes |
| Top-p | Yes |
| Max tokens | Yes |
| Presence penalty | No |
| Frequency penalty | No |
| Tool calling | No |

### Backend Comparison

| Feature | llama.cpp (`provider-local-llama`) | mistral.rs (`provider-local-mistralrs`) |
|---|---|---|
| Maturity | Mature upstream backend | Experimental |
| Language | C++ with Rust bindings | Pure Rust (Candle) |
| Build time | Fast (~30s) | Slow (~3-5 min, pulls Candle) |
| GPU support | Metal, CUDA, Vulkan | Metal, CUDA (via Candle) |
| JSON mode | No | Yes (ISQ support) |
| Chat templates | Manual | Auto-detected from GGUF metadata |
| PagedAttention | No | Yes (CUDA, Apple Silicon) |
| In-situ quantization | No | Yes (load unquantized, quantize at runtime) |
| Binary size impact | Small (~2 MB) | Large (~20 MB, Candle framework) |

---

## HTTP-Based Providers

HTTP-based local providers connect to a separately running inference server. No Cargo feature flags are needed — they use the standard HTTP transport.

### Ollama

Run models locally via [Ollama](https://ollama.com/).

```json
{
  "provider_type": "ollama",
  "base_url": "http://localhost:11434",
  "timeout_ms": 120000
}
```

**Environment variable:** `OLLAMA_HOST` (optional, defaults to `http://localhost:11434`)

**Supported models:** Any model pulled via `ollama pull`, e.g., `llama3.1`, `codellama`, `mistral`, `phi3`

**Capabilities:** System messages, streaming

**Note:** No API key required. The provider connects to a locally running Ollama server. The default timeout is 120 seconds (longer than cloud providers) to accommodate model loading.

### LM Studio

Run models locally via [LM Studio](https://lmstudio.ai/).

```json
{
  "provider_type": "lmstudio",
  "base_url": "http://localhost:1234/v1",
  "timeout_ms": 120000
}
```

**Environment variable:** `LMSTUDIO_HOST` (optional, defaults to `http://localhost:1234/v1`)

**Capabilities:** System messages, streaming

**Note:** No API key required. Start the LM Studio local server before using this provider.

---

## Choosing Between Local Providers

| Consideration | In-Process (local) | Ollama | LM Studio |
|---|---|---|---|
| Setup complexity | Download a GGUF file | Install Ollama + pull model | Install LM Studio + download model |
| External server | None required | Must be running | Must be running |
| Latency | Lowest (no HTTP overhead) | Low (localhost HTTP) | Low (localhost HTTP) |
| Model lifecycle control | Full (preload/unload API) | Managed by Ollama | Managed by LM Studio |
| Memory management | Direct (cache API) | Managed by Ollama | Managed by LM Studio |
| Feature flags needed | Yes (`provider-local-llama`) | No | No |
| Build dependencies | C++ compiler (llama.cpp) | None | None |
| Best for | Embedded/library use, max control | Quick experimentation | GUI-based development |
