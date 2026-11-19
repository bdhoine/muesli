//! Just enough HTTP/1.1 for the simulator page on 127.0.0.1: one request per connection
//! (`Connection: close`), a body only with `Content-Length`. The simulator is a development tool
//! and never ships; this keeps it free of a server framework.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Other,
}

pub struct Request {
    pub method: Method,
    /// Without the query string.
    pub path: String,
    body: Vec<u8>,
    stream: TcpStream,
}

impl Request {
    pub fn read(stream: TcpStream) -> Option<Self> {
        let mut reader = BufReader::new(stream.try_clone().ok()?);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let mut parts = line.split_whitespace();
        let method = match parts.next()? {
            "GET" => Method::Get,
            "POST" => Method::Post,
            _ => Method::Other,
        };
        let target = parts.next()?.to_string();
        let mut length = 0usize;
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header).ok()? == 0 || header.trim().is_empty() {
                break;
            }
            if let Some((name, value)) = header.split_once(':') {
                if name.trim().eq_ignore_ascii_case("content-length") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
        }
        let mut body = vec![0; length.min(1 << 20)];
        reader.read_exact(&mut body).ok()?;
        let path = target.split('?').next().unwrap_or("/").to_string();
        Some(Self { method, path, body, stream })
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or(serde_json::Value::Null)
    }

    pub fn respond(mut self, status: u16, content_type: &str, body: &[u8]) {
        let reason = match status {
            200 => "OK",
            400 => "Bad Request",
            403 => "Forbidden",
            _ => "Not Found",
        };
        let head = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = self.stream.write_all(head.as_bytes());
        let _ = self.stream.write_all(body);
        let _ = self.stream.flush();
    }
}
