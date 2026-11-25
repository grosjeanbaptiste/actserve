//! Rotates API credentials end to end: fresh values, the live database
//! password when it is one of them, the encrypted secret, its persistence, and
//! the redeploy. Same contract as `DeployService`: the host is checked first,
//! local files and every secret are read before anything changes, the run stops
//! at the first failure, `dry_run` changes nothing. No value is ever printed.
use std::path::Path;
use std::sync::Arc;

use crate::application::{
    deploy_service::DeployService,
    fleet_status_service::probe_status,
    harden_service::HardenRun,
    item_runner::{describe, failed, find_host, push},
    remote_env::with_remote_env_value,
};
use crate::domain::{
    deploy::{API_ENV_PATH, API_SECRET, GHCR_SECRET},
    errors::DomainError,
    fleet::{FleetHost, HostStatus},
    harden::{ItemOutcome, ItemReport},
    rotate::{POSTGRES_KEY, RotateTarget, alter_user_command, alter_user_sql, credentials},
    secret::SecretValue,
};
use crate::ports::outbound::{
    hostname_probe::HostnameProbePort, random::RandomPort, remote_shell::RemoteShellPort,
    secret_store::SecretStorePort, secret_writer::SecretWriterPort,
    server_inventory::ServerInventoryPort, state_persist::StatePersistPort,
};

pub struct RotatePorts {
    pub inventory: Arc<dyn ServerInventoryPort>,
    pub probe: Arc<dyn HostnameProbePort>,
    pub shell: Arc<dyn RemoteShellPort>,
    pub secrets: Arc<dyn SecretStorePort>,
    pub writer: Arc<dyn SecretWriterPort>,
    pub random: Arc<dyn RandomPort>,
    pub persist: Arc<dyn StatePersistPort>,
}

pub struct RotateService {
    ports: RotatePorts,
    deploy: Arc<DeployService>,
    db_user: String,
}

type Values = Vec<(String, SecretValue)>;

/// `text` with every occurrence of a new value hidden.
fn scrub(text: String, values: &Values) -> String {
    values
        .iter()
        .fold(text, |t, (_, v)| t.replace(v.expose(), "***"))
}

impl RotateService {
    pub fn new(ports: RotatePorts, deploy: Arc<DeployService>, db_user: impl Into<String>) -> Self {
        Self {
            ports,
            deploy,
            db_user: db_user.into(),
        }
    }

    pub async fn rotate(
        &self,
        name: &str,
        target: RotateTarget,
        api_dir: &Path,
        dry_run: bool,
    ) -> Result<HardenRun, DomainError> {
        let host = find_host(&*self.ports.inventory, name).await?;
        if let status @ (HostStatus::Unreachable { .. } | HostStatus::WrongHost { .. }) =
            probe_status(&*self.ports.probe, &host).await
        {
            return Ok(HardenRun::Refused(status));
        }
        if target.touches_database() {
            alter_user_command(&self.db_user)?;
        }
        // A dry deploy checks the host and the local files, and changes nothing.
        let preflight = match self.deploy.deploy(name, api_dir, true).await? {
            HardenRun::Refused(status) => return Ok(HardenRun::Refused(status)),
            HardenRun::Done(items) => items,
        };
        let mut items = Vec::new();
        if dry_run {
            for item in plan(target) {
                push(&mut items, "rotate", item, ItemOutcome::WouldApply);
            }
            items.extend(preflight);
            return Ok(HardenRun::Done(items));
        }
        self.run(name, target, api_dir, &host, &mut items).await;
        Ok(HardenRun::Done(items))
    }

    async fn run(
        &self,
        name: &str,
        target: RotateTarget,
        api_dir: &Path,
        host: &FleetHost,
        items: &mut Vec<ItemReport>,
    ) {
        let env = match self.read_secrets().await {
            Ok(env) => env,
            Err(e) => return fail(items, "secrets", describe(&e)),
        };
        let values = match self.generate(target) {
            Ok(values) => values,
            Err(e) => return fail(items, "generate", describe(&e)),
        };
        if let Some((_, password)) = values.iter().find(|(k, _)| k == POSTGRES_KEY)
            && !self
                .change_database(host, &env, password, &values, items)
                .await
        {
            return;
        }
        let on_failure = if target.touches_database() {
            "; the database password may already be rotated: investigate before deploying"
        } else {
            ""
        };
        let written = self.ports.writer.set_values(API_SECRET, &values).await;
        if let Err(e) = written {
            return fail(items, "secret", format!("{}{on_failure}", describe(&e)));
        }
        push(items, "rotate", "secret", ItemOutcome::Applied);
        self.persist(items).await;
        self.redeploy(name, api_dir, items).await;
    }

    /// Both secrets are read first: a locked one stops the run before any change.
    async fn read_secrets(&self) -> Result<SecretValue, DomainError> {
        self.ports.secrets.get(GHCR_SECRET).await?;
        self.ports.secrets.get(API_SECRET).await
    }

    fn generate(&self, target: RotateTarget) -> Result<Values, DomainError> {
        credentials(target)
            .into_iter()
            .map(|c| {
                let raw = self.ports.random.bytes(c.bytes)?;
                Ok((c.key.to_string(), SecretValue::new(c.render(&raw))))
            })
            .collect()
    }

    /// Changes the live password first; false when the run must stop.
    async fn change_database(
        &self,
        host: &FleetHost,
        env: &SecretValue,
        password: &SecretValue,
        values: &Values,
        items: &mut Vec<ItemReport>,
    ) -> bool {
        let sent = async {
            let sql = alter_user_sql(&self.db_user, password.expose())?;
            let command = alter_user_command(&self.db_user)?;
            with_remote_env_value(
                &*self.ports.shell,
                host.ssh_target(),
                env,
                API_ENV_PATH,
                &command,
                Some(&sql),
            )
            .await
        };
        let outcome = match sent.await {
            Ok(out) if out.code == Some(0) => ItemOutcome::Applied,
            Ok(out) => failed_database(scrub(failed(&out), values)),
            Err(e) => failed_database(scrub(describe(&e), values)),
        };
        push(items, "rotate", "database", outcome)
    }

    /// A failed persist is reported but never stops the redeploy: the database
    /// and the secret have changed, and the running API needs the new env.
    async fn persist(&self, items: &mut Vec<ItemReport>) {
        let outcome = match self.ports.persist.persist().await {
            Ok(()) => ItemOutcome::Applied,
            Err(e) => ItemOutcome::Failed(format!(
                "{}: run `machine autocommit` to back the secret up",
                describe(&e)
            )),
        };
        push(items, "rotate", "persist", outcome);
    }

    async fn redeploy(&self, name: &str, api_dir: &Path, items: &mut Vec<ItemReport>) {
        match self.deploy.deploy(name, api_dir, false).await {
            Ok(HardenRun::Done(done)) => items.extend(done),
            Ok(HardenRun::Refused(status)) => fail(
                items,
                "deploy",
                format!("host refused after the rotation: {status:?}"),
            ),
            Err(e) => fail(items, "deploy", describe(&e)),
        }
    }
}

fn plan(target: RotateTarget) -> Vec<&'static str> {
    let database = target.touches_database().then_some("database");
    database.into_iter().chain(["secret", "persist"]).collect()
}

fn failed_database(reason: String) -> ItemOutcome {
    ItemOutcome::Failed(format!("{reason}; secret untouched, nothing deployed"))
}

fn fail(items: &mut Vec<ItemReport>, item: &'static str, reason: String) {
    push(items, "rotate", item, ItemOutcome::Failed(reason));
}
