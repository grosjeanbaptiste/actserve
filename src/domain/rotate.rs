//! The credential rotation of the API: which keys change, how their values are
//! made, and the command that changes the database password on the server.
use crate::domain::{
    deploy::compose,
    encoding::{base64, hex},
    errors::DomainError,
    observe::Stack,
};

pub const DEFAULT_DB_USER: &str = "acteble_prod";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotateTarget {
    Jwt,
    Postgres,
    Minio,
    All,
}

impl RotateTarget {
    pub fn parse(word: &str) -> Result<Self, DomainError> {
        match word {
            "jwt" => Ok(Self::Jwt),
            "postgres" => Ok(Self::Postgres),
            "minio" => Ok(Self::Minio),
            "all" => Ok(Self::All),
            other => Err(DomainError::ValidationError(format!(
                "rotate: unknown target {other:?} (expected jwt|postgres|minio|all)"
            ))),
        }
    }

    /// True when the live database password must be changed too.
    pub fn touches_database(self) -> bool {
        matches!(self, Self::Postgres | Self::All)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    Base64,
    Hex,
}

/// One key of the prod env and how a fresh value is made for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Credential {
    pub key: &'static str,
    /// Random bytes to draw.
    pub bytes: usize,
    form: Form,
}

impl Credential {
    pub fn render(&self, raw: &[u8]) -> String {
        match self.form {
            Form::Base64 => base64(raw),
            Form::Hex => hex(raw),
        }
    }
}

pub const POSTGRES_KEY: &str = "POSTGRES_PASSWORD";
const JWT: Credential = Credential {
    key: "APP_AUTH__JWT_SECRET",
    bytes: 64,
    form: Form::Base64,
};
const POSTGRES: Credential = Credential {
    key: POSTGRES_KEY,
    bytes: 32,
    form: Form::Hex,
};
const MINIO_USER: Credential = Credential {
    key: "MINIO_ROOT_USER",
    bytes: 12,
    form: Form::Hex,
};
const MINIO_PASS: Credential = Credential {
    key: "MINIO_ROOT_PASSWORD",
    bytes: 32,
    form: Form::Hex,
};

pub fn credentials(target: RotateTarget) -> Vec<Credential> {
    match target {
        RotateTarget::Jwt => vec![JWT],
        RotateTarget::Postgres => vec![POSTGRES],
        RotateTarget::Minio => vec![MINIO_USER, MINIO_PASS],
        RotateTarget::All => vec![JWT, POSTGRES, MINIO_USER, MINIO_PASS],
    }
}

/// A plain SQL identifier: letter or `_` first, then letters, digits, `_`.
pub fn valid_db_user(user: &str) -> bool {
    let mut chars = user.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn refuse_user(user: &str) -> Result<(), DomainError> {
    if valid_db_user(user) {
        return Ok(());
    }
    Err(DomainError::ValidationError(format!(
        "rotate: invalid database user {user:?}"
    )))
}

/// The statement sent to psql on stdin. A lowercase hex password cannot leave
/// its quotes; anything else is refused (the error never carries it).
pub fn alter_user_sql(user: &str, password: &str) -> Result<String, DomainError> {
    refuse_user(user)?;
    let hex_only = !password.is_empty()
        && password
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
    if !hex_only {
        return Err(DomainError::ValidationError(
            "rotate: the database password is not lowercase hex".into(),
        ));
    }
    Ok(format!("ALTER USER {user} WITH PASSWORD '{password}';\n"))
}

/// Runs psql in the postgres container; reads the statement on stdin and exits
/// non-zero when it fails (`ON_ERROR_STOP`), so a refused change is never a success.
pub fn alter_user_command(user: &str) -> Result<String, DomainError> {
    refuse_user(user)?;
    Ok(format!(
        "cd '{dir}' && {psql}",
        dir = Stack::Api.dir(),
        psql = compose(&format!(
            "exec -T postgres psql -U '{user}' -v ON_ERROR_STOP=1 -q"
        )),
    ))
}
