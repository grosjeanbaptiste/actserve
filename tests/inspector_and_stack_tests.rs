use actserve::domain::{
    errors::DomainError,
    repository::{DetectedAdminType, DiscoveredVariable, RepoAdminManifest, RepoId},
    stack::{MultiRepoStack, StackId, StackRepoEntry},
};
use std::collections::HashMap;

#[test]
fn test_repo_admin_manifest_detection_types() {
    let manifest = RepoAdminManifest::new(
        DetectedAdminType::PodmanKube,
        vec!["pod.yaml".to_string(), "pod.prod.yaml".to_string()],
        vec![
            DiscoveredVariable::new("POD_NAME", Some("actserve-app".to_string())),
            DiscoveredVariable::new("PORT", Some("8080".to_string())),
        ],
    )
    .expect("valid manifest");

    assert_eq!(manifest.admin_type(), DetectedAdminType::PodmanKube);
    assert_eq!(manifest.entrypoints().len(), 2);
    assert_eq!(manifest.discovered_variables().len(), 2);
    assert!(manifest.is_container_oriented());
}

#[test]
fn test_multi_repo_stack_creation_and_step_generation() {
    let stack_id = StackId::new();
    let repo_1 = RepoId::new();
    let repo_2 = RepoId::new();

    let mut overrides_1 = HashMap::new();
    overrides_1.insert("ssh_port".to_string(), "2222".to_string());

    let entries = vec![
        StackRepoEntry {
            repo_id: repo_1,
            order: 1,
            step_name: "base-hardening".to_string(),
            command: "ansible-playbook -i inventory hardening.yml".to_string(),
            rollback_command: Some(
                "ansible-playbook -i inventory rollback-hardening.yml".to_string(),
            ),
            overrides: overrides_1,
        },
        StackRepoEntry {
            repo_id: repo_2,
            order: 2,
            step_name: "monitoring-agent".to_string(),
            command: "podman kube play --replace pod.yaml".to_string(),
            rollback_command: Some("podman kube down pod.yaml".to_string()),
            overrides: HashMap::new(),
        },
    ];

    let stack = MultiRepoStack::new(
        stack_id,
        "full-production-baseline".to_string(),
        "Hardening + Monitoring stacked administration".to_string(),
        entries,
    )
    .expect("valid multi repo stack");

    assert_eq!(stack.id(), stack_id);
    assert_eq!(stack.entries().len(), 2);

    let steps = stack.compile_to_orchestration_steps();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].name, "base-hardening");
    assert_eq!(
        steps[0].rollback_command.as_deref(),
        Some("ansible-playbook -i inventory rollback-hardening.yml")
    );
    assert_eq!(steps[1].name, "monitoring-agent");
    assert_eq!(
        steps[1].rollback_command.as_deref(),
        Some("podman kube down pod.yaml")
    );
}

#[test]
fn test_multi_repo_stack_validation_empty_entries() {
    assert!(matches!(
        MultiRepoStack::new(
            StackId::new(),
            "empty-stack".to_string(),
            "desc".to_string(),
            vec![],
        ),
        Err(DomainError::ValidationError(_))
    ));
}
