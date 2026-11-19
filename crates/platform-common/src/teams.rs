//! Is Microsoft Teams running? Asked every few seconds by the meeting detector.

/// Process names that mean Microsoft Teams is running.
pub fn is_teams_process(name: &str) -> bool {
    let n = name.to_lowercase();
    matches!(n.as_str(), "ms-teams.exe" | "teams.exe" | "msteams" | "microsoft teams" | "teams" | "ms-teams")
}

/// Looks for a running Teams process. Keeps its process table between calls and refreshes only
/// what it needs (names).
pub struct ProcessScanner {
    system: Option<sysinfo::System>,
}

impl ProcessScanner {
    pub fn new() -> Self {
        Self { system: sysinfo::System::new().ok() }
    }

    pub fn teams_running(&mut self) -> bool {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate};
        let Some(system) = self.system.as_mut() else { return false };
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
        system.processes().values().any(|p| is_teams_process(&p.name().to_string_lossy()))
    }
}

impl Default for ProcessScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_the_teams_process_names() {
        assert!(is_teams_process("ms-teams.exe"));
        assert!(is_teams_process("Microsoft Teams"));
        assert!(!is_teams_process("zoom.us"));
    }
}
