use sqlmig::{AppliedMigration, PlanError, Registry};

fn registry() -> Registry {
    Registry::build(&[
        ("0001_create_users.up.sql", "CREATE TABLE users (id INTEGER);"),
        ("0001_create_users.down.sql", "DROP TABLE users;"),
        ("0002_add_email.up.sql", "ALTER TABLE users ADD email TEXT;"),
        ("0002_add_email.down.sql", "ALTER TABLE users DROP email;"),
        ("0003_add_index.up.sql", "CREATE INDEX idx ON users (email);"),
    ])
    .expect("fixture registry should build")
}

#[test]
fn plan_apply_returns_only_unapplied_versions_in_order() {
    let reg = registry();

    let plan = reg.plan_apply(&[1]).expect("plan should succeed");
    let versions: Vec<u64> = plan.pending.iter().map(|m| m.version).collect();
    assert_eq!(versions, vec![2, 3]);
}

#[test]
fn plan_apply_with_nothing_applied_returns_everything() {
    let reg = registry();

    let plan = reg.plan_apply(&[]).expect("plan should succeed");
    let versions: Vec<u64> = plan.pending.iter().map(|m| m.version).collect();
    assert_eq!(versions, vec![1, 2, 3]);
}

#[test]
fn plan_apply_with_everything_applied_returns_nothing() {
    let reg = registry();

    let plan = reg.plan_apply(&[1, 2, 3]).expect("plan should succeed");
    assert!(plan.pending.is_empty());
}

#[test]
fn plan_apply_rejects_an_applied_version_missing_from_the_registry() {
    let reg = registry();

    let result = reg.plan_apply(&[1, 99]);
    assert_eq!(
        result,
        Err(PlanError::AppliedVersionNotInRegistry { version: 99 })
    );
}

#[test]
fn plan_rollback_returns_applied_versions_above_target_descending() {
    let reg = registry();

    let rollback = reg
        .plan_rollback(&[1, 2], 0)
        .expect("plan should succeed");
    let versions: Vec<u64> = rollback.iter().map(|m| m.version).collect();
    assert_eq!(versions, vec![2, 1]);
}

#[test]
fn plan_rollback_stops_at_target() {
    let reg = registry();

    let rollback = reg
        .plan_rollback(&[1, 2], 1)
        .expect("plan should succeed");
    let versions: Vec<u64> = rollback.iter().map(|m| m.version).collect();
    assert_eq!(versions, vec![2]);
}

#[test]
fn plan_rollback_ignores_pending_versions_above_target() {
    let reg = registry();

    // Version 3 is never applied, so it has nothing to roll back.
    let rollback = reg
        .plan_rollback(&[1, 2], 0)
        .expect("plan should succeed");
    assert!(rollback.iter().all(|m| m.version != 3));
}

#[test]
fn plan_rollback_rejects_a_version_with_no_down_sql() {
    let reg = registry();

    // Version 3 has no down file. Applying it and asking to roll it back
    // past target 0 should fail rather than silently skip it.
    let result = reg.plan_rollback(&[1, 2, 3], 0);
    assert_eq!(result, Err(PlanError::MissingDownSql { version: 3 }));
}

#[test]
fn plan_rollback_rejects_an_applied_version_missing_from_the_registry() {
    let reg = registry();

    let result = reg.plan_rollback(&[1, 42], 0);
    assert_eq!(
        result,
        Err(PlanError::AppliedVersionNotInRegistry { version: 42 })
    );
}

#[test]
fn check_drift_accepts_matching_checksums() {
    let reg = registry();
    let up_checksum = reg.migrations[0].up_checksum;

    let result = reg.check_drift(&[AppliedMigration {
        version: 1,
        up_checksum,
    }]);
    assert_eq!(result, Ok(()));
}

#[test]
fn check_drift_catches_an_edited_up_file() {
    let reg = registry();
    let current = reg.migrations[0].up_checksum;

    let result = reg.check_drift(&[AppliedMigration {
        version: 1,
        up_checksum: current.wrapping_add(1),
    }]);
    assert_eq!(
        result,
        Err(PlanError::ChecksumMismatch {
            version: 1,
            recorded: current.wrapping_add(1),
            current,
        })
    );
}

#[test]
fn check_drift_rejects_an_applied_version_missing_from_the_registry() {
    let reg = registry();

    let result = reg.check_drift(&[AppliedMigration {
        version: 99,
        up_checksum: 0,
    }]);
    assert_eq!(
        result,
        Err(PlanError::AppliedVersionNotInRegistry { version: 99 })
    );
}

#[test]
fn check_drift_ignores_versions_never_reported_as_applied() {
    let reg = registry();

    // Only version 1 is checked; versions 2 and 3 aren't mentioned at all.
    let result = reg.check_drift(&[AppliedMigration {
        version: 1,
        up_checksum: reg.migrations[0].up_checksum,
    }]);
    assert_eq!(result, Ok(()));
}
