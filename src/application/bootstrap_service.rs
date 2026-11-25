use std::sync::Arc;

use crate::application::{
    fleet_audit_service::FleetAuditService,
    harden_service::{HardenRun, HardenService},
    item_runner::find_host,
    runtime_service::RuntimeService,
    stacks_service::StacksService,
};
use crate::domain::{
    dirs_file::parse_dirs,
    errors::DomainError,
    fleet_audit::{AuditRun, AuditVerdict},
    harden::{ItemOutcome, ItemReport},
    ports_file::parse_ports,
    stacks::parse_stacks,
};
use crate::ports::outbound::{
    declared_data::DeclaredDataPort, server_inventory::ServerInventoryPort,
};

/// Brings a server to its declared state, one step at a time: reach, harden,
/// runtime, edge, stacks, audit. Every declared file is validated before the
/// first remote call; each step is skipped when its checks already pass; the
/// run stops at the first failed item; the audit comes last and decides.
pub struct BootstrapService {
    inventory: Arc<dyn ServerInventoryPort>,
    data: Arc<dyn DeclaredDataPort>,
    harden: Arc<HardenService>,
    runtime: Arc<RuntimeService>,
    stacks: Arc<StacksService>,
    audit: Arc<FleetAuditService>,
}

/// Adds a step's items to the run; false when the run must stop there.
fn absorb(all: &mut Vec<ItemReport>, step: HardenRun) -> Result<bool, HardenRun> {
    match step {
        HardenRun::Refused(_) => Err(step),
        HardenRun::Done(items) => {
            let go = !items
                .iter()
                .any(|i| matches!(i.outcome, ItemOutcome::Failed(_)));
            all.extend(items);
            Ok(go)
        }
    }
}

impl BootstrapService {
    pub fn new(
        inventory: Arc<dyn ServerInventoryPort>,
        data: Arc<dyn DeclaredDataPort>,
        harden: Arc<HardenService>,
        runtime: Arc<RuntimeService>,
        stacks: Arc<StacksService>,
        audit: Arc<FleetAuditService>,
    ) -> Self {
        Self {
            inventory,
            data,
            harden,
            runtime,
            stacks,
            audit,
        }
    }

    /// Every declared file must be readable and valid, so a typo in step 5's
    /// file never surfaces after step 2 already changed the server.
    async fn validate(&self, name: &str) -> Result<(), DomainError> {
        find_host(&*self.inventory, name).await?;
        self.data.read(name, "sshd.conf").await?;
        parse_ports("ports.txt", &self.data.read(name, "ports.txt").await?)?;
        parse_dirs("dirs.txt", &self.data.read(name, "dirs.txt").await?)?;
        parse_stacks("stacks.txt", &self.data.read(name, "stacks.txt").await?)?;
        Ok(())
    }

    async fn step(&self, step: Step, name: &str, dry_run: bool) -> Result<HardenRun, DomainError> {
        match step {
            Step::Harden => self.harden.run(name, dry_run).await,
            Step::Runtime => self.runtime.run(name, dry_run).await,
            Step::Edge => self.stacks.edge(name, dry_run).await,
            Step::Stacks => self.stacks.stacks(name, dry_run).await,
        }
    }

    /// With `dry_run` nothing is applied and the audit is not run (it would
    /// judge a server that was not changed).
    pub async fn run(&self, name: &str, dry_run: bool) -> Result<HardenRun, DomainError> {
        self.validate(name).await?;
        let mut all = Vec::new();
        for step in [Step::Harden, Step::Runtime, Step::Edge, Step::Stacks] {
            match absorb(&mut all, self.step(step, name, dry_run).await?) {
                Ok(true) => {}
                Ok(false) => return Ok(HardenRun::Done(all)),
                Err(refused) => return Ok(refused),
            }
        }
        if !dry_run {
            // The server may have stopped answering as itself during the run
            // (an sshd reload): that is reported as the refusal it is.
            match self.audit.audit(name).await? {
                AuditRun::Refused(status) => return Ok(HardenRun::Refused(status)),
                done @ AuditRun::Done(_) => all.push(ItemReport {
                    step: "audit",
                    item: "audit",
                    outcome: audit_item(&done),
                }),
            }
        }
        Ok(HardenRun::Done(all))
    }
}

#[derive(Clone, Copy)]
enum Step {
    Harden,
    Runtime,
    Edge,
    Stacks,
}

fn audit_item(run: &AuditRun) -> ItemOutcome {
    match run {
        AuditRun::Done(r) => match &r.verdict {
            AuditVerdict::Conforming => ItemOutcome::Ok,
            AuditVerdict::Deviation => ItemOutcome::Failed("deviation".into()),
            AuditVerdict::Unreadable { reason } => {
                ItemOutcome::Failed(format!("unreadable: {reason}"))
            }
        },
        AuditRun::Refused(_) => ItemOutcome::Failed("not audited".into()),
    }
}
