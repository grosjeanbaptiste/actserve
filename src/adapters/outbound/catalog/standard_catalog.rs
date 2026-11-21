use async_trait::async_trait;

use crate::domain::{
    catalog::{AdminCategory, CatalogItem, CatalogItemSpec, ConfigVariable},
    errors::DomainError,
    repository::{GitRef, RepoKind},
};
use crate::ports::outbound::repo_catalog::RepoCatalogPort;

pub struct StandardServerAdminCatalog {
    items: Vec<CatalogItem>,
}

impl Default for StandardServerAdminCatalog {
    fn default() -> Self {
        let items = vec![
            CatalogItem::new(CatalogItemSpec { id: "linux-hardening".into(), name: "Linux OS & SSH Security Hardening".into(), description: "Dev-Sec CIS-benchmark compliant system hardening for Linux servers (SSH, kernel sysctl, auditd, limits).".into(), category: AdminCategory::Security, upstream_url: "https://github.com/dev-sec/ansible-collection-hardening.git".into(), target_ref: GitRef::Branch("master".to_string()), kind: RepoKind::Playbook, supported_os: vec!["Ubuntu".to_string(), "Debian".to_string(), "RHEL".to_string()], required_variables: vec![
                    ConfigVariable::new("ssh_port", "Custom SSH port", Some("22".to_string()), false),
                    ConfigVariable::new("allow_root_ssh", "Permit root login over SSH", Some("no".to_string()), false),
                ], }).unwrap(),
            CatalogItem::new(CatalogItemSpec { id: "docker-engine-compose".into(), name: "Docker Engine & Docker Compose".into(), description: "Production-ready Docker engine installation with daemon configuration, log rotation, and Compose plugin.".into(), category: AdminCategory::Containers, upstream_url: "https://github.com/geerlingguy/ansible-role-docker.git".into(), target_ref: GitRef::Branch("master".to_string()), kind: RepoKind::Playbook, supported_os: vec!["Ubuntu".to_string(), "Debian".to_string(), "RHEL".to_string(), "Alpine".to_string()], required_variables: vec![
                    ConfigVariable::new("docker_storage_driver", "Storage driver (overlay2)", Some("overlay2".to_string()), false),
                ], }).unwrap(),
            CatalogItem::new(CatalogItemSpec { id: "prometheus-node-exporter".into(), name: "Prometheus Node Exporter & Monitoring".into(), description: "Prometheus host metrics collector exposing hardware and OS metrics (CPU, memory, disk, network).".into(), category: AdminCategory::Monitoring, upstream_url: "https://github.com/prometheus-community/ansible-node-exporter.git".into(), target_ref: GitRef::Branch("main".to_string()), kind: RepoKind::Playbook, supported_os: vec!["Ubuntu".to_string(), "Debian".to_string(), "RHEL".to_string()], required_variables: vec![
                    ConfigVariable::new("exporter_port", "HTTP metrics port", Some("9100".to_string()), false),
                ], }).unwrap(),
            CatalogItem::new(CatalogItemSpec { id: "fail2ban-ufw-security".into(), name: "Fail2ban & UFW Firewall Protection".into(), description: "Automatic IP banning on SSH brute-force attempts and persistent UFW firewall configuration.".into(), category: AdminCategory::Security, upstream_url: "https://github.com/geerlingguy/ansible-role-security.git".into(), target_ref: GitRef::Branch("master".to_string()), kind: RepoKind::Playbook, supported_os: vec!["Ubuntu".to_string(), "Debian".to_string()], required_variables: vec![
                    ConfigVariable::new("ban_time", "Ban duration in seconds", Some("3600".to_string()), false),
                    ConfigVariable::new("max_retry", "Maximum login retries before ban", Some("5".to_string()), false),
                ], }).unwrap(),
            CatalogItem::new(CatalogItemSpec { id: "traefik-proxy".into(), name: "Traefik v3 Edge Router & Reverse Proxy".into(), description: "High-performance cloud-native edge router with automated Let's Encrypt SSL/TLS certificates.".into(), category: AdminCategory::Networking, upstream_url: "https://github.com/traefik/traefik.git".into(), target_ref: GitRef::Branch("master".to_string()), kind: RepoKind::DockerCompose, supported_os: vec![], required_variables: vec![
                    ConfigVariable::new("acme_email", "Email for Let's Encrypt certificates", None, false),
                    ConfigVariable::new("domain", "Primary server domain name", None, false),
                ], }).unwrap(),
            CatalogItem::new(CatalogItemSpec { id: "wireguard-vpn".into(), name: "WireGuard High-Speed Secure VPN".into(), description: "Extremely fast and modern VPN server using state-of-the-art cryptography for secure remote management.".into(), category: AdminCategory::Networking, upstream_url: "https://github.com/angristan/wireguard-install.git".into(), target_ref: GitRef::Branch("master".to_string()), kind: RepoKind::ShellManifest, supported_os: vec!["Ubuntu".to_string(), "Debian".to_string()], required_variables: vec![
                    ConfigVariable::new("vpn_port", "WireGuard UDP listen port", Some("51820".to_string()), false),
                ], }).unwrap(),
            CatalogItem::new(CatalogItemSpec { id: "borgbackup-server".into(), name: "BorgBackup Encrypted Deduplicating Backup".into(), description: "Deduplicating backup tool with authenticated encryption and compression for server data retention.".into(), category: AdminCategory::Backup, upstream_url: "https://github.com/borgbackup/borg.git".into(), target_ref: GitRef::Branch("master".to_string()), kind: RepoKind::Playbook, supported_os: vec!["Ubuntu".to_string(), "Debian".to_string()], required_variables: vec![
                    ConfigVariable::new("passphrase", "Encryption passphrase", None, true),
                ], }).unwrap(),
            CatalogItem::new(CatalogItemSpec { id: "caddy-webserver".into(), name: "Caddy Web Server with Automatic HTTPS".into(), description: "Fast, memory-safe HTTP/2 & HTTP/3 reverse proxy and static web server with fully automated SSL.".into(), category: AdminCategory::Web, upstream_url: "https://github.com/caddyserver/caddy.git".into(), target_ref: GitRef::Branch("master".to_string()), kind: RepoKind::RawConfig, supported_os: vec!["Ubuntu".to_string(), "Debian".to_string()], required_variables: vec![
                    ConfigVariable::new("domain", "Domain name", None, false),
                ], }).unwrap(),
            CatalogItem::new(CatalogItemSpec { id: "dotfiles-stow".into(), name: "Dotfiles & Environment Provisioning (GNU Stow)".into(), description: "Automated symlink management for zsh, starship, git, and developer machine environment via GNU Stow.".into(), category: AdminCategory::Security, upstream_url: "https://github.com/grosjeanbaptiste/dotfiles.git".into(), target_ref: GitRef::Branch("main".to_string()), kind: RepoKind::ShellManifest, supported_os: vec!["Ubuntu".to_string(), "Debian".to_string(), "Darwin".to_string()], required_variables: vec![
                    ConfigVariable::new("stow_packages", "Packages to stow (comma-separated)", Some("starship,zsh,git".to_string()), false),
                ], }).unwrap(),
            CatalogItem::new(CatalogItemSpec { id: "ss-sensor".into(), name: "System Status & Telemetry Sensor (ss)".into(), description: "High-precision modular telemetry agent (kernel, network sockets, memory RSS/swap, processes, storage I/O).".into(), category: AdminCategory::Monitoring, upstream_url: "https://github.com/grosjeanbaptiste/ss.git".into(), target_ref: GitRef::Branch("main".to_string()), kind: RepoKind::RawConfig, supported_os: vec!["Ubuntu".to_string(), "Debian".to_string(), "Darwin".to_string()], required_variables: vec![
                    ConfigVariable::new("metrics_interval_secs", "Telemetry collection frequency", Some("5".to_string()), false),
                ], }).unwrap(),
        ];

        Self { items }
    }
}

impl StandardServerAdminCatalog {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl RepoCatalogPort for StandardServerAdminCatalog {
    async fn list_catalog(&self) -> Result<Vec<CatalogItem>, DomainError> {
        Ok(self.items.clone())
    }

    async fn find_catalog_item(&self, id: &str) -> Result<Option<CatalogItem>, DomainError> {
        Ok(self.items.iter().find(|i| i.id() == id).cloned())
    }
}
