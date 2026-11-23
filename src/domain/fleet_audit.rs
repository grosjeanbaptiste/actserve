use crate::domain::fleet::HostStatus;

/// What `edge audit` said about a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditVerdict {
    Conforming,
    Deviation,
    /// The audit could not be read: ssh failed or `edge` crashed.
    Unreadable {
        reason: String,
    },
}

/// The verdict and what the audit printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditReport {
    pub verdict: AuditVerdict,
    pub output: String,
}

/// An audit that ran, or the reason the host was not audited at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditRun {
    /// Unreachable or another host: nothing was audited.
    Refused(HostStatus),
    Done(AuditReport),
}
