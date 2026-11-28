use actserve::domain::{
    catalog::AdminCategory,
    registry::{
        AdminPackage, AdminPackageSpec, RegistryQuery, RegistrySource, ServerProfilePreset,
    },
    server::ServerSpec,
};

#[test]
fn test_admin_package_creation_and_dependency_resolution() {
    let pkg = AdminPackage::new(
        AdminPackageSpec { id: "traefik-edge-router".into(), name: "Traefik v3 Reverse Proxy".into(), description: "Automated edge router with Let's Encrypt certificates".into(), source: RegistrySource::LinuxServerIo, repo_url: "https://github.com/traefik/traefik.git".into(), target_ref: "master".into(), category: AdminCategory::Networking, dependencies: vec!["podman-engine".to_string()], supported_os: // depends on docker
        vec!["Ubuntu".to_string(), "Debian".to_string()], stars: 15000, },
    )
    .expect("valid package");

    assert_eq!(pkg.id(), "traefik-edge-router");
    assert_eq!(pkg.source(), RegistrySource::LinuxServerIo);
    assert_eq!(pkg.dependencies(), &["podman-engine"]);
    assert_eq!(pkg.stars(), 15000);

    let spec = ServerSpec {
        os: "Debian 12 Bookworm".to_string(),
        arch: "x86_64".to_string(),
        cpu_cores: 4,
        ram_mb: 8192,
    };
    assert!(pkg.is_compatible_with(&spec));
}

#[test]
fn test_registry_query_filtering() {
    let query = RegistryQuery::builder()
        .keyword("security")
        .category(AdminCategory::Security)
        .source(RegistrySource::AnsibleGalaxy)
        .min_stars(100)
        .build();

    assert_eq!(query.keyword.as_deref(), Some("security"));
    assert_eq!(query.category, Some(AdminCategory::Security));
    assert_eq!(query.source, Some(RegistrySource::AnsibleGalaxy));
    assert_eq!(query.min_stars, Some(100));

    let pkg_match = AdminPackage::new(AdminPackageSpec {
        id: "dev-sec-hardening".into(),
        name: "Dev-Sec Hardening".into(),
        description: "Security hardening collection".into(),
        source: RegistrySource::AnsibleGalaxy,
        repo_url: "https://github.com/dev-sec/ansible-collection-hardening.git".into(),
        target_ref: "main".into(),
        category: AdminCategory::Security,
        dependencies: vec![],
        supported_os: vec![],
        stars: 1200,
    })
    .unwrap();

    assert!(query.matches(&pkg_match));

    let pkg_no_match = AdminPackage::new(AdminPackageSpec {
        id: "grafana".into(),
        name: "Grafana Dashboards".into(),
        description: "Monitoring visualization".into(),
        source: RegistrySource::LinuxServerIo,
        repo_url: "https://github.com/grafana/grafana.git".into(),
        target_ref: "main".into(),
        category: AdminCategory::Monitoring,
        dependencies: vec![],
        supported_os: vec![],
        stars: 5000,
    })
    .unwrap();

    assert!(!query.matches(&pkg_no_match));
}

#[test]
fn test_server_profile_preset_composition() {
    let preset = ServerProfilePreset::new(
        "hardened-web-server",
        "Production Hardened Web Server",
        "Combines OS hardening, firewall, Podman and Caddy HTTPS reverse proxy",
        vec![
            "dev-sec-hardening".to_string(),
            "ufw-fail2ban".to_string(),
            "podman-engine".to_string(),
            "caddy-proxy".to_string(),
        ],
    )
    .expect("valid preset");

    assert_eq!(preset.id(), "hardened-web-server");
    assert_eq!(preset.package_ids().len(), 4);
    assert_eq!(preset.package_ids()[0], "dev-sec-hardening");
    assert_eq!(preset.package_ids()[3], "caddy-proxy");
}

#[test]
fn test_gibs_developer_node_preset_composition() {
    let preset = ServerProfilePreset::new(
        "gibs-developer-node",
        "GIBS Developer Node",
        "Hardened server + Podman + Dotfiles (Stow) + System Status (ss) sensor + WireGuard VPN",
        vec![
            "dev-sec-hardening".to_string(),
            "containers-podman".to_string(),
            "dotfiles-stow".to_string(),
            "ss-sensor".to_string(),
            "linuxserver-wireguard".to_string(),
        ],
    )
    .expect("valid preset");

    assert_eq!(preset.id(), "gibs-developer-node");
    assert_eq!(preset.package_ids().len(), 5);
    assert!(preset.package_ids().contains(&"dotfiles-stow".to_string()));
    assert!(preset.package_ids().contains(&"ss-sensor".to_string()));
}
