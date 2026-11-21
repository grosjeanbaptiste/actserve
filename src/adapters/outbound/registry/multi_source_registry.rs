use async_trait::async_trait;

use crate::domain::{
    catalog::AdminCategory,
    errors::DomainError,
    registry::{
        AdminPackage, AdminPackageSpec, RegistryQuery, RegistrySource, ServerProfilePreset,
    },
};
use crate::ports::outbound::admin_registry::AdminRegistryPort;

pub struct MultiSourceAdminRegistry {
    packages: Vec<AdminPackage>,
    presets: Vec<ServerProfilePreset>,
}

impl Default for MultiSourceAdminRegistry {
    fn default() -> Self {
        let packages = vec![
            AdminPackage::new(AdminPackageSpec { id: "dev-sec-hardening".into(), name: "Dev-Sec OS & SSH Hardening".into(), description: "CIS-benchmark baseline hardening for Linux (kernel parameters, limits, sshd security).".into(), source: RegistrySource::AnsibleGalaxy, repo_url: "https://github.com/dev-sec/ansible-collection-hardening.git".into(), target_ref: "master".into(), category: AdminCategory::Security, dependencies: vec![], supported_os: vec!["Ubuntu".to_string(), "Debian".to_string(), "RHEL".to_string()], stars: 2100, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "geerlingguy-security".into(), name: "Fail2ban & UFW Firewall".into(), description: "Automated firewall policies and dynamic brute-force attacker IP banning.".into(), source: RegistrySource::AnsibleGalaxy, repo_url: "https://github.com/geerlingguy/ansible-role-security.git".into(), target_ref: "master".into(), category: AdminCategory::Security, dependencies: vec![], supported_os: vec!["Ubuntu".to_string(), "Debian".to_string()], stars: 1800, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "geerlingguy-docker".into(), name: "Docker Engine & Compose".into(), description: "Enterprise Docker engine setup with daemon configurations, storage driver and Compose plugin.".into(), source: RegistrySource::AnsibleGalaxy, repo_url: "https://github.com/geerlingguy/ansible-role-docker.git".into(), target_ref: "master".into(), category: AdminCategory::Containers, dependencies: vec![], supported_os: vec!["Ubuntu".to_string(), "Debian".to_string(), "RHEL".to_string()], stars: 5400, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "geerlingguy-nginx".into(), name: "Nginx Web & Reverse Proxy".into(), description: "High-performance Nginx server configured for optimal HTTP performance, SSL termination and reverse proxying.".into(), source: RegistrySource::AnsibleGalaxy, repo_url: "https://github.com/geerlingguy/ansible-role-nginx.git".into(), target_ref: "master".into(), category: AdminCategory::Web, dependencies: vec![], supported_os: vec!["Ubuntu".to_string(), "Debian".to_string()], stars: 3200, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "geerlingguy-postgresql".into(), name: "PostgreSQL Database Server".into(), description: "Production relational database setup with tuning for server memory, users and automated backups.".into(), source: RegistrySource::AnsibleGalaxy, repo_url: "https://github.com/geerlingguy/ansible-role-postgresql.git".into(), target_ref: "master".into(), category: AdminCategory::Database, dependencies: vec![], supported_os: vec!["Ubuntu".to_string(), "Debian".to_string()], stars: 2900, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "geerlingguy-certbot".into(), name: "Let's Encrypt Certbot".into(), description: "Automated TLS/SSL certificate issuance and renewal via Let's Encrypt.".into(), source: RegistrySource::AnsibleGalaxy, repo_url: "https://github.com/geerlingguy/ansible-role-certbot.git".into(), target_ref: "master".into(), category: AdminCategory::Security, dependencies: vec![], supported_os: vec!["Ubuntu".to_string(), "Debian".to_string()], stars: 2500, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "prometheus-node-exporter".into(), name: "Prometheus Node Exporter".into(), description: "Host telemetry agent measuring CPU, RAM, disk I/O, network bandwidth and temperature.".into(), source: RegistrySource::GitHubCommunity, repo_url: "https://github.com/prometheus-community/ansible-node-exporter.git".into(), target_ref: "main".into(), category: AdminCategory::Monitoring, dependencies: vec![], supported_os: vec!["Ubuntu".to_string(), "Debian".to_string(), "RHEL".to_string()], stars: 1100, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "netdata-agent".into(), name: "Netdata Real-Time Performance Agent".into(), description: "High-fidelity second-by-second metrics monitor with automated anomaly detection.".into(), source: RegistrySource::GitHubCommunity, repo_url: "https://github.com/netdata/netdata.git".into(), target_ref: "master".into(), category: AdminCategory::Monitoring, dependencies: vec![], supported_os: vec!["Ubuntu".to_string(), "Debian".to_string()], stars: 71000, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "linuxserver-wireguard".into(), name: "WireGuard VPN Server".into(), description: "Fast, modern, cryptographic VPN server packaged by LinuxServer.io.".into(), source: RegistrySource::LinuxServerIo, repo_url: "https://github.com/linuxserver/docker-wireguard.git".into(), target_ref: "master".into(), category: AdminCategory::Networking, dependencies: vec!["geerlingguy-docker".to_string()], supported_os: vec![], stars: 4500, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "linuxserver-nextcloud".into(), name: "Nextcloud Private Cloud Suite".into(), description: "Private cloud file sync, calendar, contacts and office collaboration platform.".into(), source: RegistrySource::LinuxServerIo, repo_url: "https://github.com/linuxserver/docker-nextcloud.git".into(), target_ref: "master".into(), category: AdminCategory::Storage, dependencies: vec!["geerlingguy-docker".to_string()], supported_os: vec![], stars: 3800, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "linuxserver-swag".into(), name: "SWAG - Secure Web Application Gateway".into(), description: "Nginx reverse proxy with built-in Certbot, PHP support and fail2ban intrusion prevention.".into(), source: RegistrySource::LinuxServerIo, repo_url: "https://github.com/linuxserver/docker-swag.git".into(), target_ref: "master".into(), category: AdminCategory::Networking, dependencies: vec!["geerlingguy-docker".to_string()], supported_os: vec![], stars: 2900, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "traefik-proxy".into(), name: "Traefik v3 Edge Proxy".into(), description: "Cloud-native reverse proxy and load balancer with automatic Let's Encrypt certificates.".into(), source: RegistrySource::GitHubCommunity, repo_url: "https://github.com/traefik/traefik.git".into(), target_ref: "master".into(), category: AdminCategory::Networking, dependencies: vec!["geerlingguy-docker".to_string()], supported_os: vec![], stars: 53000, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "caddy-proxy".into(), name: "Caddy Web Server with Automatic TLS".into(), description: "Enterprise HTTPS web server and reverse proxy with zero-configuration SSL/TLS.".into(), source: RegistrySource::GitHubCommunity, repo_url: "https://github.com/caddyserver/caddy.git".into(), target_ref: "master".into(), category: AdminCategory::Web, dependencies: vec![], supported_os: vec!["Ubuntu".to_string(), "Debian".to_string()], stars: 58000, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "borgbackup".into(), name: "BorgBackup Deduplicating Backup".into(), description: "Space-efficient, encrypted, authenticated and compressed offsite backups for servers.".into(), source: RegistrySource::CuratedServai, repo_url: "https://github.com/borgbackup/borg.git".into(), target_ref: "master".into(), category: AdminCategory::Backup, dependencies: vec![], supported_os: vec!["Ubuntu".to_string(), "Debian".to_string()], stars: 11000, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "crowdsec".into(), name: "CrowdSec Collaborative Intrusion Prevention".into(), description: "Modern security engine parsing logs to detect and block malicious traffic via community threat intel.".into(), source: RegistrySource::GitHubCommunity, repo_url: "https://github.com/crowdsecurity/crowdsec.git".into(), target_ref: "master".into(), category: AdminCategory::Security, dependencies: vec![], supported_os: vec!["Ubuntu".to_string(), "Debian".to_string()], stars: 10500, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "restic-backup".into(), name: "Restic Fast Secure Backup".into(), description: "Secure backups supporting cloud object storage (S3, B2, Azure, SFTP) out of the box.".into(), source: RegistrySource::GitHubCommunity, repo_url: "https://github.com/restic/restic.git".into(), target_ref: "master".into(), category: AdminCategory::Backup, dependencies: vec![], supported_os: vec!["Ubuntu".to_string(), "Debian".to_string()], stars: 28000, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "k3s-cluster".into(), name: "K3s Lightweight Kubernetes".into(), description: "Highly available certified lightweight Kubernetes cluster orchestrator for edge and cloud servers.".into(), source: RegistrySource::GitHubCommunity, repo_url: "https://github.com/k3s-io/k3s-ansible.git".into(), target_ref: "master".into(), category: AdminCategory::Containers, dependencies: vec![], supported_os: vec!["Ubuntu".to_string(), "Debian".to_string()], stars: 8500, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "vaultwarden-server".into(), name: "Vaultwarden Password Manager".into(), description: "Lightweight Bitwarden-compatible password and secret management server.".into(), source: RegistrySource::GitHubCommunity, repo_url: "https://github.com/dani-garcia/vaultwarden.git".into(), target_ref: "main".into(), category: AdminCategory::Security, dependencies: vec!["geerlingguy-docker".to_string()], supported_os: vec![], stars: 39000, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "uptime-kuma".into(), name: "Uptime Kuma Service Monitor".into(), description: "Self-hosted status page and uptime monitoring tool with multi-channel alerts.".into(), source: RegistrySource::GitHubCommunity, repo_url: "https://github.com/louislam/uptime-kuma.git".into(), target_ref: "master".into(), category: AdminCategory::Monitoring, dependencies: vec!["geerlingguy-docker".to_string()], supported_os: vec![], stars: 66000, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "grafana-server".into(), name: "Grafana Observability Dashboards".into(), description: "Open-source data visualization and metrics dashboarding platform.".into(), source: RegistrySource::GitHubCommunity, repo_url: "https://github.com/grafana/grafana.git".into(), target_ref: "main".into(), category: AdminCategory::Monitoring, dependencies: vec!["geerlingguy-docker".to_string()], supported_os: vec![], stars: 63000, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "dotfiles-stow".into(), name: "Dotfiles & Machine State Provisioning".into(), description: "Personalized dotfiles, shell environment, and GNU Stow manifests.".into(), source: RegistrySource::GitHubCommunity, repo_url: "https://github.com/grosjeanbaptiste/dotfiles.git".into(), target_ref: "main".into(), category: AdminCategory::Security, dependencies: vec![], supported_os: vec!["Ubuntu".to_string(), "Debian".to_string(), "Darwin".to_string()], stars: 50, }).unwrap(),
            AdminPackage::new(AdminPackageSpec { id: "ss-sensor".into(), name: "SS High-Precision System Sensor".into(), description: "Modular diagnostic and administration sensor for kernel, network, memory, and processes.".into(), source: RegistrySource::GitHubCommunity, repo_url: "https://github.com/grosjeanbaptiste/ss.git".into(), target_ref: "main".into(), category: AdminCategory::Monitoring, dependencies: vec![], supported_os: vec!["Ubuntu".to_string(), "Debian".to_string(), "Darwin".to_string()], stars: 100, }).unwrap(),
        ];

        let presets = vec![
            ServerProfilePreset::new(
                "secure-web-baseline",
                "Secure Web Baseline",
                "Production Linux hardening + UFW/Fail2ban firewall + Caddy auto-HTTPS reverse proxy.",
                vec![
                    "dev-sec-hardening".to_string(),
                    "geerlingguy-security".to_string(),
                    "caddy-proxy".to_string(),
                ],
            ).unwrap(),
            ServerProfilePreset::new(
                "docker-application-host",
                "Docker Application Host",
                "Security hardening + Docker engine & Compose + Traefik v3 edge reverse proxy.",
                vec![
                    "dev-sec-hardening".to_string(),
                    "geerlingguy-security".to_string(),
                    "geerlingguy-docker".to_string(),
                    "traefik-proxy".to_string(),
                ],
            ).unwrap(),
            ServerProfilePreset::new(
                "observability-node",
                "Full Observability & Monitoring Node",
                "Prometheus Node Exporter + Netdata real-time agent + Grafana visual dashboards.",
                vec![
                    "prometheus-node-exporter".to_string(),
                    "netdata-agent".to_string(),
                    "geerlingguy-docker".to_string(),
                    "grafana-server".to_string(),
                ],
            ).unwrap(),
            ServerProfilePreset::new(
                "homelab-private-cloud",
                "Homelab & Private Cloud Suite",
                "Hardened server + Docker + WireGuard VPN + Nextcloud files + Vaultwarden vault.",
                vec![
                    "dev-sec-hardening".to_string(),
                    "geerlingguy-docker".to_string(),
                    "linuxserver-wireguard".to_string(),
                    "linuxserver-nextcloud".to_string(),
                    "vaultwarden-server".to_string(),
                ],
            ).unwrap(),
            ServerProfilePreset::new(
                "gibs-developer-node",
                "GIBS Developer Node",
                "Hardened server + Docker + Dotfiles (Stow) + System Status (ss) sensor + WireGuard VPN.",
                vec![
                    "dev-sec-hardening".to_string(),
                    "geerlingguy-docker".to_string(),
                    "dotfiles-stow".to_string(),
                    "ss-sensor".to_string(),
                    "linuxserver-wireguard".to_string(),
                ],
            ).unwrap(),
        ];

        Self { packages, presets }
    }
}

impl MultiSourceAdminRegistry {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl AdminRegistryPort for MultiSourceAdminRegistry {
    async fn search(&self, query: &RegistryQuery) -> Result<Vec<AdminPackage>, DomainError> {
        let results = self
            .packages
            .iter()
            .filter(|pkg| query.matches(pkg))
            .cloned()
            .collect();
        Ok(results)
    }

    async fn get_package(&self, id: &str) -> Result<Option<AdminPackage>, DomainError> {
        Ok(self.packages.iter().find(|p| p.id() == id).cloned())
    }

    async fn list_presets(&self) -> Result<Vec<ServerProfilePreset>, DomainError> {
        Ok(self.presets.clone())
    }

    async fn resolve_dependencies(
        &self,
        package_id: &str,
    ) -> Result<Vec<AdminPackage>, DomainError> {
        let root = self
            .get_package(package_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity: "AdminPackage",
                id: package_id.to_string(),
            })?;

        let mut resolved = Vec::new();
        for dep_id in root.dependencies() {
            let dep_pkg = self
                .get_package(dep_id)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity: "AdminPackage dependency",
                    id: dep_id.clone(),
                })?;
            if !resolved
                .iter()
                .any(|p: &AdminPackage| p.id() == dep_pkg.id())
            {
                resolved.push(dep_pkg);
            }
        }
        resolved.push(root);

        Ok(resolved)
    }
}
