use std::sync::Arc;

use muesli_core::ports::Trash;
use muesli_core::{Paths, VERSION};
use serde_json::{json, Map, Value};

use crate::tools::Tools;

pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

type Refusal = Box<dyn Fn() -> Option<String> + Send + Sync>;
type Audit = Box<dyn Fn(&str, Option<&str>) + Send + Sync>;

/// JSON-RPC 2.0 in, JSON-RPC 2.0 out. Transport-agnostic: stdio feeds `handle` line by line.
pub struct Server {
    tools: Tools,
    refusal: Refusal,
    audit: Audit,
}

impl Server {
    pub fn new(paths: Paths, trash: Arc<dyn Trash>) -> Self {
        Self { tools: Tools::new(paths, trash), refusal: Box::new(|| None), audit: Box::new(|_, _| {}) }
    }

    /// A message when tool calls must be refused (integration switched off).
    pub fn with_refusal(mut self, refusal: impl Fn() -> Option<String> + Send + Sync + 'static) -> Self {
        self.refusal = Box::new(refusal);
        self
    }

    /// Called for every tool call: tool name, meeting id if any.
    pub fn with_audit(mut self, audit: impl Fn(&str, Option<&str>) + Send + Sync + 'static) -> Self {
        self.audit = Box::new(audit);
        self
    }

    pub fn tools_mut(&mut self) -> &mut Tools {
        &mut self.tools
    }

    /// One request body or line. Returns `None` when nothing has to be sent back (notifications).
    pub fn handle(&self, data: &[u8]) -> Option<Vec<u8>> {
        let Ok(json) = serde_json::from_slice::<Value>(data) else {
            return Some(encode(&error(Value::Null, -32700, "Parse error")));
        };
        match json {
            Value::Array(batch) => {
                let replies: Vec<Value> = batch.iter().filter_map(|m| m.as_object().and_then(|m| self.handle_one(m))).collect();
                (!replies.is_empty()).then(|| encode(&Value::Array(replies)))
            }
            Value::Object(message) => self.handle_one(&message).map(|v| encode(&v)),
            // Swift's JSONSerialization refuses top-level scalars, so they are parse errors there too.
            _ => Some(encode(&error(Value::Null, -32700, "Parse error"))),
        }
    }

    fn handle_one(&self, msg: &Map<String, Value>) -> Option<Value> {
        let id = msg.get("id").filter(|id| !id.is_null())?.clone(); // notification otherwise
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            return if msg.contains_key("result") || msg.contains_key("error") { None } else { Some(error(id, -32600, "Invalid request")) };
        };
        let empty = Map::new();
        let params = msg.get("params").and_then(Value::as_object).unwrap_or(&empty);
        match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
                let version = if PROTOCOL_VERSIONS.contains(&asked) { asked } else { PROTOCOL_VERSIONS[0] };
                Some(result(
                    id,
                    json!({
                        "protocolVersion": version,
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "muesli", "title": "Muesli", "version": VERSION },
                        "instructions": crate::tools::instructions(),
                    }),
                ))
            }
            "ping" => Some(result(id, json!({}))),
            "tools/list" => Some(result(id, json!({ "tools": Tools::definitions() }))),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).filter(|n| Tools::is_tool(n));
                let Some(name) = name else { return Some(error(id, -32602, "Unknown tool")) };
                let args = params.get("arguments").and_then(Value::as_object).unwrap_or(&empty);
                if let Some(refused) = (self.refusal)() {
                    return Some(result(id, tool_result(&refused, true)));
                }
                (self.audit)(name, args.get("id").and_then(Value::as_str));
                Some(match self.tools.call(name, args) {
                    Ok(text) => result(id, tool_result(&text, false)),
                    Err(e) => result(id, tool_result(&e.message, true)),
                })
            }
            _ => Some(error(id, -32601, "Method not found")),
        }
    }
}

fn tool_result(text: &str, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

fn result(id: Value, value: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": value })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Compact JSON with sorted keys (serde_json's default map), like the Swift server.
fn encode(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec())
}
