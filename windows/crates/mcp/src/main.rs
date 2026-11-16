use std::io::{BufRead, Write};

use muesli_core::{Paths, Preferences, VERSION};
use muesli_mcp::{audit, doctor, Server, Tools};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let paths = Paths::from_env();
    match args.first().map(String::as_str) {
        None | Some("stdio") => serve(paths),
        Some("doctor") => std::process::exit(if doctor::run(&paths) { 0 } else { 1 }),
        Some("--version") => println!("muesli-mcp {VERSION}"),
        Some("--mcpb-manifest") => {
            let binary = if cfg!(windows) { "server/muesli-mcp.exe" } else { "server/muesli-mcp" };
            println!("{}", muesli_core::assistants::ClaudeDesktopExtension::manifest_json(VERSION, binary, "icon.png"));
        }
        Some(other) => {
            println!(
                "muesli-mcp {VERSION}: MCP server for Muesli\n\n  muesli-mcp                  serve over stdio (Claude Desktop, Claude Code and other MCP clients)\n  muesli-mcp doctor           check the setup"
            );
            std::process::exit(if other == "--help" || other == "-h" { 0 } else { 2 });
        }
    }
}

fn serve(paths: Paths) {
    let refusal_paths = paths.clone();
    let audit_paths = paths.clone();
    let server = Server::new(paths)
        .with_refusal(move || {
            if Preferences::load(&refusal_paths).mcp_enabled {
                None
            } else {
                Some("AI assistant access is switched off in Muesli. Turn it on in Muesli > Settings > AI Assistants.".into())
            }
        })
        .with_audit(move |tool, meeting_id| {
            if Tools::MANAGE_NAMES.contains(&tool) {
                audit::record(&audit_paths, tool, meeting_id);
            }
        });

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }
        if let Some(reply) = server.handle(line.as_bytes()) {
            let _ = stdout.write_all(&reply);
            let _ = stdout.write_all(b"\n");
            let _ = stdout.flush();
        }
    }
}
