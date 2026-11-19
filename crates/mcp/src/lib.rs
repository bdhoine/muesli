//! Minimal Model Context Protocol server for Muesli: JSON-RPC 2.0 over stdio.
//! Same tools, schemas and wording as the Swift `muesli-mcp`.

pub mod audit;
pub mod doctor;
pub mod server;
pub mod tools;

pub use server::Server;
pub use tools::{ToolError, Tools};

pub use muesli_core::trash_name;
