use serde::{Deserialize, Serialize};

use crate::domain::{
    errors::DomainError,
    repository::{GitRef, RepoKind},
    server::ServerSpec,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdminCategory {
    Security,
    Monitoring,
    Networking,
    Containers,
    Backup,
    Web,
    Database,
    Storage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigVariable {
    pub name: String,
    pub description: String,
    pub default_value: Option<String>,
    pub is_secret: bool,
}

impl ConfigVariable {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        default_value: Option<String>,
        is_secret: bool,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            default_value,
            is_secret,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogItem {
    id: String,
    name: String,
    description: String,
    category: AdminCategory,
    upstream_url: String,
    target_ref: GitRef,
    kind: RepoKind,
    supported_os: Vec<String>,
    required_variables: Vec<ConfigVariable>,
}

/// Raw, unvalidated fields of a [`CatalogItem`]; `CatalogItem::new` validates them.
#[derive(Debug, Clone)]
pub struct CatalogItemSpec {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: AdminCategory,
    pub upstream_url: String,
    pub target_ref: GitRef,
    pub kind: RepoKind,
    pub supported_os: Vec<String>,
    pub required_variables: Vec<ConfigVariable>,
}

impl CatalogItem {
    pub fn new(spec: CatalogItemSpec) -> Result<Self, DomainError> {
        let CatalogItemSpec {
            id,
            name,
            description,
            category,
            upstream_url,
            target_ref,
            kind,
            supported_os,
            required_variables,
        } = spec;
        let id_s: String = id;
        let trimmed_id = id_s.trim();
        if trimmed_id.is_empty() {
            return Err(DomainError::ValidationError(
                "Catalog item id cannot be empty".to_string(),
            ));
        }

        let name_s: String = name;
        let trimmed_name = name_s.trim();
        if trimmed_name.is_empty() {
            return Err(DomainError::ValidationError(
                "Catalog item name cannot be empty".to_string(),
            ));
        }

        let url_s: String = upstream_url;
        let trimmed_url = url_s.trim();
        if !trimmed_url.starts_with("git@")
            && !trimmed_url.starts_with("https://")
            && !trimmed_url.starts_with("http://")
            && !trimmed_url.starts_with("ssh://")
            && !trimmed_url.starts_with("file://")
        {
            return Err(DomainError::ValidationError(
                "Upstream URL must be a valid git/https/ssh/file URL".to_string(),
            ));
        }

        Ok(Self {
            id: trimmed_id.to_string(),
            name: trimmed_name.to_string(),
            description,
            category,
            upstream_url: trimmed_url.to_string(),
            target_ref,
            kind,
            supported_os,
            required_variables,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn category(&self) -> AdminCategory {
        self.category
    }

    pub fn upstream_url(&self) -> &str {
        &self.upstream_url
    }

    pub fn target_ref(&self) -> &GitRef {
        &self.target_ref
    }

    pub fn kind(&self) -> RepoKind {
        self.kind
    }

    pub fn supported_os(&self) -> &[String] {
        &self.supported_os
    }

    pub fn required_variables(&self) -> &[ConfigVariable] {
        &self.required_variables
    }

    pub fn is_compatible_with(&self, spec: &ServerSpec) -> bool {
        if self.supported_os.is_empty() {
            return true;
        }

        let spec_os = spec.os.to_lowercase();
        self.supported_os
            .iter()
            .any(|os| spec_os.contains(&os.to_lowercase()))
    }
}
