use crate::domain::errors::DomainError;

/// One line of `servers/<name>/dirs.txt`: a directory and the user owning it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredDir {
    pub path: String,
    pub owner: String,
}

/// The directories of `servers/<name>/dirs.txt` (`ABSOLUTE-PATH|OWNER`), in
/// file order. Blank lines and `#` lines are skipped. A path is absolute, made
/// of letters, digits and `_ . / -`, without a `..` component; an owner is a
/// user name. Both end up inside a shell command, so nothing else gets
/// through. Every problem is reported in one pass as `<file>:<line>: …`.
pub fn parse_dirs(file: &str, text: &str) -> Result<Vec<DeclaredDir>, DomainError> {
    let mut dirs = Vec::new();
    let mut problems = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match line.split_once('|') {
            Some((path, owner)) if valid_path(path) && valid_owner(owner) => {
                dirs.push(DeclaredDir {
                    path: path.into(),
                    owner: owner.into(),
                });
            }
            _ => problems.push(format!(
                "{file}:{}: \"{line}\" must be ABSOLUTE-PATH|OWNER (no .. component)",
                i + 1
            )),
        }
    }
    if problems.is_empty() {
        Ok(dirs)
    } else {
        Err(DomainError::ValidationError(problems.join("\n")))
    }
}

fn valid_path(path: &str) -> bool {
    path.len() > 1
        && path.starts_with('/')
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_./-".contains(&b))
        && !path.split('/').any(|part| part == "..")
}

fn valid_owner(owner: &str) -> bool {
    let mut bytes = owner.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_lowercase() || b == b'_')
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}
