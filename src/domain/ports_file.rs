use crate::domain::errors::DomainError;

/// The rules of `servers/<name>/ports.txt`, as `NUMBER/PROTOCOL` in file order.
/// Blank lines and `#` lines are skipped. The file must let 22/tcp in:
/// enabling the firewall without it would lock the operator out. Every
/// problem is reported in one pass as `<file>:<line>: …`.
pub fn parse_ports(file: &str, text: &str) -> Result<Vec<String>, DomainError> {
    let mut rules = Vec::new();
    let mut problems = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if valid_rule(line) {
            rules.push(line.to_string());
        } else {
            problems.push(format!(
                "{file}:{}: \"{line}\" must be NUMBER/tcp or NUMBER/udp (1-65535)",
                i + 1
            ));
        }
    }
    if problems.is_empty() && !rules.iter().any(|r| r == "22/tcp") {
        problems.push(format!(
            "{file}: must allow 22/tcp, or enabling the firewall would lock the operator out"
        ));
    }
    if problems.is_empty() {
        Ok(rules)
    } else {
        Err(DomainError::ValidationError(problems.join("\n")))
    }
}

fn valid_rule(line: &str) -> bool {
    let Some((number, proto)) = line.split_once('/') else {
        return false;
    };
    let in_range = number.len() <= 5
        && number.bytes().all(|b| b.is_ascii_digit())
        && number
            .parse::<u32>()
            .is_ok_and(|n| (1..=65535).contains(&n));
    in_range && (proto == "tcp" || proto == "udp")
}
