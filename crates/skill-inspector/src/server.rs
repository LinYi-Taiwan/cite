//! `serve`: a minimal std-only HTTP server on loopback (contracts/cli.md + action-api.md).
//! Serves the control-panel UI + the initial inventory export, and exposes the action
//! endpoints that are the only disk writers. Binds 127.0.0.1 only — nothing leaves the machine.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

use serde_json::Value;

use crate::action::{dispatch, ActionCtx};
use crate::export;
use crate::scan::source::{Registry, ScanContext};
use crate::scan::{self, Inventory};
use crate::state::InspectorState;
use crate::INSPECTOR_HTML;

pub struct ServeConfig {
    pub agents: Vec<String>,
    pub project_root: PathBuf,
    pub home: PathBuf,
    pub state_path: PathBuf,
    pub quarantine_dir: PathBuf,
    pub trash_dir: PathBuf,
}

impl ServeConfig {
    fn ctx(&self) -> ScanContext {
        ScanContext {
            project_root: self.project_root.clone(),
            home: self.home.clone(),
        }
    }

    fn inventory(&self) -> Inventory {
        let state = InspectorState::load_from(&self.state_path);
        let registry = Registry::with_defaults();
        scan::scan(&self.agents, &self.ctx(), &state, &registry)
    }

    fn export_json(&self) -> String {
        let inv = self.inventory();
        let export = export::build(&self.agents, &self.ctx(), inv);
        serde_json::to_string(&export).unwrap_or_else(|_| "{}".to_string())
    }

    /// Raw `SKILL.md` for a skill in the current inventory (looked up by key — never built
    /// from the request, so no path can be injected). `None` if unknown/unreadable.
    fn skill_markdown(&self, key: &str) -> Option<String> {
        let inv = self.inventory();
        let skill = inv.find(key)?;
        let md = std::path::Path::new(&skill.path).join("SKILL.md");
        std::fs::read_to_string(md).ok()
    }
}

/// Read one query parameter (percent-decoded) from a request path.
fn query_param(path: &str, name: &str) -> Option<String> {
    let query = path.split_once('?')?.1;
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k == name {
            return Some(percent_decode(v));
        }
    }
    None
}

/// Minimal application/x-www-form-urlencoded decode (`+` → space, `%XX` → byte).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let hex = |c: u8| (c as char).to_digit(16);
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                }
                _ => {
                    out.push(bytes[i]);
                    i += 1;
                }
            },
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Start the server. `port = None` picks an ephemeral port; the bound address is printed.
pub fn serve(config: ServeConfig, port: Option<u16>) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port.unwrap_or(0)))?;
    let addr = listener.local_addr()?;
    eprintln!("skill-inspector serving on http://{addr}/ (loopback only)");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(e) = handle(stream, &config) {
                    eprintln!("connection error: {e}");
                }
            }
            Err(e) => eprintln!("accept error: {e}"),
        }
    }
    Ok(())
}

struct Request {
    method: String,
    path: String,
    host: String,
    body: String,
}

/// True when the `Host` header names this loopback server (port ignored). Rejecting anything else
/// defeats DNS-rebinding and cross-origin POSTs from a browsed page: a rebound/foreign request
/// carries `evil.com`, not `localhost`. The server already binds 127.0.0.1, so the only callers
/// that legitimately reach it use a localhost Host.
fn host_is_local(host: &str) -> bool {
    let hostname = if let Some(rest) = host.strip_prefix('[') {
        // IPv6 literal `[::1]:port` → take up to `]`.
        rest.split(']').next().unwrap_or("")
    } else {
        // `host:port` → strip the port; bare `host` → unchanged.
        host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host)
    };
    matches!(hostname, "127.0.0.1" | "localhost" | "::1")
}

fn read_request(stream: &TcpStream) -> std::io::Result<Option<Request>> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(None);
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    let mut content_length = 0usize;
    let mut host = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        let lower = trimmed.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        } else if let Some(value) = lower.strip_prefix("host:") {
            host = value.trim().to_string();
        }
    }

    // Cap the body so a bogus Content-Length can't trigger a huge allocation (local DoS).
    // Action payloads are tiny; 1 MiB is generous.
    const MAX_BODY: usize = 1024 * 1024;
    if content_length > MAX_BODY {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "request body exceeds maximum",
        ));
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    Ok(Some(Request {
        method,
        path,
        host,
        body: String::from_utf8_lossy(&body).into_owned(),
    }))
}

fn handle(mut stream: TcpStream, config: &ServeConfig) -> std::io::Result<()> {
    let Some(req) = read_request(&stream)? else {
        return Ok(());
    };
    let path = req.path.split('?').next().unwrap_or("/");

    let (status, content_type, body) = match (req.method.as_str(), path) {
        ("GET", "/") | ("GET", "/index.html") => (
            "200 OK",
            "text/html; charset=utf-8",
            // Dev loop: when CITE_DEV_HTML points at the asset file, read it from disk on every
            // request so editing inspector.html shows up on a browser refresh — no rebuild needed.
            // Unset (production / installed binary) falls back to the embedded copy.
            std::env::var("CITE_DEV_HTML")
                .ok()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_else(|| INSPECTOR_HTML.to_string()),
        ),
        ("GET", "/api/inventory") => ("200 OK", "application/json", config.export_json()),
        ("GET", "/api/skill") => {
            match query_param(&req.path, "key").and_then(|k| config.skill_markdown(&k)) {
                Some(md) => ("200 OK", "text/markdown; charset=utf-8", md),
                None => ("404 Not Found", "text/plain", "skill not found".to_string()),
            }
        }
        ("POST", "/api/disable" | "/api/enable" | "/api/plugin" | "/api/remove" | "/api/label") => {
            // Mutating endpoints write settings (incl. global ~/.claude/settings.json). Refuse any
            // request whose Host isn't this loopback server — blocks DNS-rebinding / cross-origin
            // POSTs from a page the user happens to be browsing.
            if !host_is_local(&req.host) {
                (
                    "403 Forbidden",
                    "text/plain",
                    "forbidden: non-local host".to_string(),
                )
            } else {
                let action = path.trim_start_matches("/api/");
                let resp = handle_action(config, action, &req.body);
                ("200 OK", "application/json", resp.to_string())
            }
        }
        _ => ("404 Not Found", "text/plain", "not found".to_string()),
    };

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

fn handle_action(config: &ServeConfig, action: &str, body: &str) -> Value {
    let req: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return serde_json::json!({ "ok": false, "error": "invalid_json" }),
    };
    let inventory = config.inventory();
    let ctx = ActionCtx {
        inventory: &inventory,
        project_root: config.project_root.clone(),
        home: config.home.clone(),
        state_path: config.state_path.clone(),
        quarantine_dir: config.quarantine_dir.clone(),
        trash_dir: config.trash_dir.clone(),
    };
    dispatch(&ctx, action, &req)
}
