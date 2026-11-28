//! The rotation plan: targets, credentials, and the commands run on the server.
use actserve::domain::errors::DomainError;
use actserve::domain::rotate::{
    DEFAULT_DB_USER, RotateTarget, alter_user_command, alter_user_sql, credentials, valid_db_user,
};

fn keys(target: RotateTarget) -> Vec<&'static str> {
    credentials(target).iter().map(|c| c.key).collect()
}

#[test]
fn a_target_is_parsed_from_its_word() {
    assert_eq!(RotateTarget::parse("jwt").unwrap(), RotateTarget::Jwt);
    assert_eq!(
        RotateTarget::parse("postgres").unwrap(),
        RotateTarget::Postgres
    );
    assert_eq!(RotateTarget::parse("minio").unwrap(), RotateTarget::Minio);
    assert_eq!(RotateTarget::parse("all").unwrap(), RotateTarget::All);
}

#[test]
fn an_unknown_target_is_refused_naming_the_choices() {
    let Err(DomainError::ValidationError(m)) = RotateTarget::parse("redis") else {
        panic!("expected a validation error");
    };
    assert!(
        m.contains("redis") && m.contains("jwt|postgres|minio|all"),
        "{m}"
    );
}

#[test]
fn each_target_rotates_its_own_keys() {
    assert_eq!(keys(RotateTarget::Jwt), ["APP_AUTH__JWT_SECRET"]);
    assert_eq!(keys(RotateTarget::Postgres), ["POSTGRES_PASSWORD"]);
    assert_eq!(
        keys(RotateTarget::Minio),
        ["MINIO_ROOT_USER", "MINIO_ROOT_PASSWORD"]
    );
    assert_eq!(
        keys(RotateTarget::All),
        [
            "APP_AUTH__JWT_SECRET",
            "POSTGRES_PASSWORD",
            "MINIO_ROOT_USER",
            "MINIO_ROOT_PASSWORD"
        ]
    );
}

#[test]
fn only_postgres_and_all_change_the_database() {
    assert!(!RotateTarget::Jwt.touches_database());
    assert!(RotateTarget::Postgres.touches_database());
    assert!(!RotateTarget::Minio.touches_database());
    assert!(RotateTarget::All.touches_database());
}

#[test]
fn values_have_the_shape_the_bash_rotation_gave_them() {
    let all = credentials(RotateTarget::All);
    let render = |i: usize| all[i].render(&vec![0xab; all[i].bytes]);
    assert_eq!(all[0].bytes, 64);
    assert_eq!(render(0).len(), 88, "jwt: base64 of 64 bytes");
    assert_eq!(render(1), "ab".repeat(32), "postgres: 32 bytes of hex");
    assert_eq!(render(2), "ab".repeat(12), "minio user: 12 bytes of hex");
    assert_eq!(
        render(3),
        "ab".repeat(32),
        "minio password: 32 bytes of hex"
    );
}

#[test]
fn the_default_database_user_is_the_prod_one() {
    assert_eq!(DEFAULT_DB_USER, "acteble_prod");
}

#[test]
fn a_database_user_is_a_plain_identifier() {
    assert!(valid_db_user("acteble_prod"));
    assert!(valid_db_user("_u1"));
    for bad in ["", "1u", "a-b", "a b", "a'b", "a\"b", "a;b", "a\nb"] {
        assert!(!valid_db_user(bad), "{bad:?}");
    }
}

#[test]
fn the_alter_statement_carries_the_password_in_single_quotes() {
    let sql = alter_user_sql("acteble_prod", &"ab".repeat(32)).unwrap();
    assert_eq!(
        sql,
        format!(
            "ALTER USER acteble_prod WITH PASSWORD '{}';\n",
            "ab".repeat(32)
        )
    );
}

#[test]
fn a_password_that_could_leave_its_quotes_is_refused_without_echoing_it() {
    for bad in ["", "a'b", "a\\b", "a b", "a;b", "A1"] {
        let Err(DomainError::ValidationError(m)) = alter_user_sql("acteble_prod", bad) else {
            panic!("expected a validation error for {bad:?}");
        };
        assert!(!m.contains(bad) || bad.is_empty(), "{m}");
    }
}

#[test]
fn a_bad_user_is_refused_by_both_builders() {
    assert!(alter_user_sql("a'b", "ab").is_err());
    assert!(alter_user_command("a'b").is_err());
}

#[test]
fn the_command_runs_psql_in_the_postgres_container_and_stops_on_a_sql_error() {
    let cmd = alter_user_command("acteble_prod").unwrap();
    assert_eq!(
        cmd,
        "podman exec -i systemd-acteble_api-postgres psql -U 'acteble_prod' -v ON_ERROR_STOP=1 -q"
    );
}
