//! Outcomes of the hardening steps of a server.

/// What happened to one step of the hardening.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemOutcome {
    /// Check passed, nothing was changed.
    Ok,
    /// Check failed, apply ran and succeeded.
    Applied,
    /// Dry run: the check failed, apply was not run.
    WouldApply,
    Failed(String),
}

/// Result of installing the sshd drop-in over a held connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshdOutcome {
    Installed,
    /// The fresh-connection proof failed and the previous state was restored.
    RolledBack,
    Failed {
        reason: String,
    },
}

/// One reported step: its label and what happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemReport {
    pub item: &'static str,
    pub outcome: ItemOutcome,
}
