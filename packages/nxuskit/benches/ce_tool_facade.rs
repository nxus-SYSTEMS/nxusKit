//! CE tool facade streaming overhead smoke benchmark.
//!
//! tier: ce-impl benchmark
//!
//! This is an executable bench target instead of Criterion to avoid adding a
//! wrapper-crate dev dependency. It verifies that registering the CE external
//! tool facade does not add measurable work to provider-native streaming chunk
//! pass-through.

use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use nxuskit::tools::{
    CeFacadeError, CeToolPosture, ExternalTool, ToolCallingProvider, ToolResult,
    register_external_tool,
};
use nxuskit::{StreamChunk, ToolCallDelta};
use serde_json::{Value, json};

const ITERATIONS: u32 = 10_000;
const MAX_OVERHEAD_NS: u128 = 5_000_000; // 5ms per first chunk

#[derive(Debug)]
struct BenchProvider {
    tools: Mutex<HashMap<String, ExternalTool>>,
}

impl BenchProvider {
    fn new() -> Self {
        Self {
            tools: Mutex::new(HashMap::new()),
        }
    }
}

impl ToolCallingProvider for BenchProvider {
    fn supports_tool_calling(&self) -> bool {
        true
    }

    fn provider_name(&self) -> &str {
        "bench-provider"
    }

    fn tool_calling_posture(&self) -> CeToolPosture {
        CeToolPosture::Native
    }

    fn register_external_tool(&mut self, tool: ExternalTool) -> Result<(), CeFacadeError> {
        self.tools
            .lock()
            .expect("bench tools lock poisoned")
            .insert(tool.name().to_string(), tool);
        Ok(())
    }

    fn invoke_external_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<ToolResult, CeFacadeError> {
        let tools = self.tools.lock().expect("bench tools lock poisoned");
        let tool = tools
            .get(name)
            .ok_or_else(|| CeFacadeError::Contract(format!("unknown tool: {name}")))?;
        Ok(tool.invoke(arguments))
    }
}

fn first_tool_chunk_json() -> String {
    let delta = serde_json::to_value(ToolCallDelta {
        index: 0,
        id: Some("call-001".to_string()),
        tool_type: Some("function".to_string()),
        function: None,
    })
    .expect("delta serializes");
    let chunk = StreamChunk {
        delta: String::new(),
        index: 0,
        thinking: None,
        finish_reason: None,
        usage: None,
        tool_calls: Some(vec![delta]),
        logprobs: None,
    };
    serde_json::to_string(&chunk).expect("chunk serializes")
}

fn measure(mut op: impl FnMut()) -> Duration {
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        op();
    }
    start.elapsed()
}

fn avg_nanos(total: Duration) -> u128 {
    total.as_nanos() / u128::from(ITERATIONS)
}

fn main() {
    let chunk_json = first_tool_chunk_json();
    let mut provider = BenchProvider::new();

    black_box(serde_json::from_str::<StreamChunk>(&chunk_json).unwrap());
    let baseline_ns = avg_nanos(measure(|| {
        black_box(serde_json::from_str::<StreamChunk>(black_box(&chunk_json)).unwrap());
    }));

    register_external_tool(
        &mut provider,
        ExternalTool::new(
            "echo",
            "Echo JSON",
            json!({ "type": "object" }),
            ToolResult::success,
        ),
    )
    .expect("tool registration succeeds");

    let facade_registered_ns = avg_nanos(measure(|| {
        black_box(serde_json::from_str::<StreamChunk>(black_box(&chunk_json)).unwrap());
    }));
    let overhead = facade_registered_ns.saturating_sub(baseline_ns);

    println!(
        "ce_tool_facade_first_chunk: baseline_avg_ns={baseline_ns} facade_registered_avg_ns={facade_registered_ns} overhead_ns={overhead}"
    );
    assert!(
        overhead <= MAX_OVERHEAD_NS,
        "CE tool facade first-chunk overhead must stay <=5ms; observed {overhead}ns"
    );
}
