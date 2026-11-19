use muesli_core::assistants::AssistantClient;

/// Mirror of `AssistantClient` for Swift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ClientKind {
    ClaudeCode,
    Codex,
    GeminiCli,
    VsCode,
    Cursor,
    Other,
}

impl From<ClientKind> for AssistantClient {
    fn from(c: ClientKind) -> Self {
        match c {
            ClientKind::ClaudeCode => Self::ClaudeCode,
            ClientKind::Codex => Self::Codex,
            ClientKind::GeminiCli => Self::GeminiCli,
            ClientKind::VsCode => Self::VsCode,
            ClientKind::Cursor => Self::Cursor,
            ClientKind::Other => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ClientInfo {
    pub name: String,
    pub instructions: String,
    pub is_terminal_command: bool,
}

#[uniffi::export]
pub fn client_info(client: ClientKind) -> ClientInfo {
    let c = AssistantClient::from(client);
    ClientInfo { name: c.name().into(), instructions: c.instructions(), is_terminal_command: c.is_terminal_command() }
}

/// The text to copy so `client` starts the `muesli-mcp` at `helper`.
#[uniffi::export]
pub fn client_snippet(client: ClientKind, helper: String) -> String {
    AssistantClient::from(client).snippet(&helper)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_to_the_core() {
        assert_eq!(client_snippet(ClientKind::Codex, "/a b/muesli-mcp".into()), "codex mcp add muesli -- '/a b/muesli-mcp'");
        assert!(client_info(ClientKind::ClaudeCode).is_terminal_command);
    }
}
