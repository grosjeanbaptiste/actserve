use std::sync::Arc;

use crate::application::{fleet_status_service::probe_status, item_runner::find_host};
use crate::domain::{
    errors::DomainError,
    fleet::HostStatus,
    observe::{Stack, logs_command, ps_command, resources_command},
};
use crate::ports::outbound::{
    hostname_probe::HostnameProbePort,
    remote_shell::{RemoteOutput, RemoteShellPort},
    remote_terminal::RemoteTerminalPort,
    server_inventory::ServerInventoryPort,
};

/// What looking at a stack came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObserveRun {
    /// Not the expected host: nothing was run.
    Refused(HostStatus),
    /// A command that answered (`ps`).
    Output(RemoteOutput),
    /// A command that streamed to the terminal (`logs`); its exit code.
    Attached(Option<i32>),
}

/// Reads the state of a stack on a server (`ps`, `logs`). Read-only; refuses a
/// host that is not the expected one, like every other actserve command.
pub struct ObserveService {
    inventory: Arc<dyn ServerInventoryPort>,
    probe: Arc<dyn HostnameProbePort>,
    shell: Arc<dyn RemoteShellPort>,
    terminal: Arc<dyn RemoteTerminalPort>,
}

impl ObserveService {
    pub fn new(
        inventory: Arc<dyn ServerInventoryPort>,
        probe: Arc<dyn HostnameProbePort>,
        shell: Arc<dyn RemoteShellPort>,
        terminal: Arc<dyn RemoteTerminalPort>,
    ) -> Self {
        Self {
            inventory,
            probe,
            shell,
            terminal,
        }
    }

    pub async fn ps(&self, name: &str, stack: Stack) -> Result<ObserveRun, DomainError> {
        let host = find_host(&*self.inventory, name).await?;
        let status = probe_status(&*self.probe, &host).await;
        if status != HostStatus::Ok {
            return Ok(ObserveRun::Refused(status));
        }
        let out = self
            .shell
            .run(host.ssh_target(), &ps_command(stack), None)
            .await?;
        Ok(ObserveRun::Output(out))
    }

    /// What the whole server is using. Read-only, like `ps`.
    pub async fn resources(&self, name: &str) -> Result<ObserveRun, DomainError> {
        let host = find_host(&*self.inventory, name).await?;
        let status = probe_status(&*self.probe, &host).await;
        if status != HostStatus::Ok {
            return Ok(ObserveRun::Refused(status));
        }
        let out = self
            .shell
            .run(host.ssh_target(), resources_command(), None)
            .await?;
        Ok(ObserveRun::Output(out))
    }

    pub async fn logs(
        &self,
        name: &str,
        stack: Stack,
        service: Option<&str>,
    ) -> Result<ObserveRun, DomainError> {
        let command = logs_command(stack, service.unwrap_or(stack.default_service()))?;
        let host = find_host(&*self.inventory, name).await?;
        let status = probe_status(&*self.probe, &host).await;
        if status != HostStatus::Ok {
            return Ok(ObserveRun::Refused(status));
        }
        let code = self.terminal.attach(host.ssh_target(), &command).await?;
        Ok(ObserveRun::Attached(code))
    }
}

/// 3 refused, else the command's own exit code (1 when a signal killed it).
pub fn exit_code(run: &ObserveRun) -> u8 {
    let code = match run {
        ObserveRun::Refused(_) => return 3,
        ObserveRun::Output(o) => o.code,
        ObserveRun::Attached(c) => *c,
    };
    code.and_then(|c| u8::try_from(c).ok()).unwrap_or(1)
}

/// What goes to stdout: the answer of `ps`, the refusal, nothing for a stream.
pub fn render(name: &str, run: &ObserveRun) -> String {
    match run {
        ObserveRun::Refused(HostStatus::WrongHost { got, expected }) => {
            format!("{name}: wrong-host (answered {got}, expected {expected})\n")
        }
        ObserveRun::Refused(HostStatus::Unreachable { reason }) => {
            format!("{name}: unreachable ({reason})\n")
        }
        ObserveRun::Refused(HostStatus::Ok) | ObserveRun::Attached(_) => String::new(),
        ObserveRun::Output(o) => o.stdout.clone(),
    }
}
