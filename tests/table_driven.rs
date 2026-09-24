use sqlmig::{parse_filename, BuildError, Direction, ParseError, Registry};

#[test]
fn parse_filename_cases() {
    struct Case {
        input: &'static str,
        expected: Result<(u64, &'static str, Direction), ParseError>,
    }

    let cases = [
        Case {
            input: "0001_create_users.up.sql",
            expected: Ok((1, "create_users", Direction::Up)),
        },
        // Leading zeros must normalize to the same version as no padding.
        Case {
            input: "1_create_users.up.sql",
            expected: Ok((1, "create_users", Direction::Up)),
        },
        Case {
            input: "0001_create_users.down.sql",
            expected: Ok((1, "create_users", Direction::Down)),
        },
        // A leading underscore in the name is fine, just unusual.
        Case {
            input: "0001__create.up.sql",
            expected: Ok((1, "_create", Direction::Up)),
        },
        // Version zero is reserved for "nothing applied yet" and can't be
        // claimed by a migration.
        Case {
            input: "0_init.up.sql",
            expected: Err(ParseError::ZeroVersion),
        },
        // No numeric prefix at all.
        Case {
            input: "create_users.up.sql",
            expected: Err(ParseError::InvalidVersion),
        },
        // No underscore, so there's no name segment to split off.
        Case {
            input: "0001.up.sql",
            expected: Err(ParseError::MissingName),
        },
        // Direction is case sensitive.
        Case {
            input: "0001_create_users.UP.sql",
            expected: Err(ParseError::InvalidDirection),
        },
        // Extension is case sensitive too.
        Case {
            input: "0001_create_users.up.SQL",
            expected: Err(ParseError::InvalidExtension),
        },
        // Missing the direction segment entirely.
        Case {
            input: "0001_create_users.sql",
            expected: Err(ParseError::InvalidFormat),
        },
        Case {
            input: "",
            expected: Err(ParseError::InvalidFormat),
        },
        // Extra dots in the name spill a "." into what should be the name.
        Case {
            input: "0001_create_users.up.down.sql",
            expected: Err(ParseError::InvalidName),
        },
        Case {
            input: "0001_create-users.up.sql",
            expected: Err(ParseError::InvalidName),
        },
        Case {
            input: "-1_create.up.sql",
            expected: Err(ParseError::InvalidVersion),
        },
        // Digits, but too large to fit in a u64.
        Case {
            input: "99999999999999999999_create.up.sql",
            expected: Err(ParseError::InvalidVersion),
        },
        // Underscore immediately before the direction leaves an empty name.
        Case {
            input: "0001_.up.sql",
            expected: Err(ParseError::InvalidName),
        },
    ];

    for case in cases {
        let actual = parse_filename(case.input).map(|p| (p.version, p.name, p.direction));
        match (actual, case.expected) {
            (Ok((v, n, d)), Ok((ev, en, ed))) => {
                assert_eq!(v, ev, "version mismatch for {:?}", case.input);
                assert_eq!(n, en, "name mismatch for {:?}", case.input);
                assert_eq!(d, ed, "direction mismatch for {:?}", case.input);
            }
            (Err(e), Err(ee)) => {
                assert_eq!(e, ee, "error mismatch for {:?}", case.input);
            }
            (actual, expected) => panic!(
                "mismatch for {:?}: got {:?}, want {:?}",
                case.input, actual, expected
            ),
        }
    }
}

#[test]
fn registry_build_cases() {
    struct Case {
        label: &'static str,
        files: &'static [(&'static str, &'static str)],
        check: fn(Result<Registry, BuildError>),
    }

    let cases: [Case; 9] = [
        Case {
            label: "matched up/down pair",
            files: &[
                ("0001_create_users.up.sql", "CREATE TABLE users (id INTEGER);"),
                ("0001_create_users.down.sql", "DROP TABLE users;"),
            ],
            check: |result| {
                let registry = result.expect("expected build to succeed");
                assert_eq!(registry.migrations.len(), 1);
                let m = &registry.migrations[0];
                assert_eq!(m.version, 1);
                assert_eq!(m.name, "create_users");
                assert_eq!(m.down_sql.as_deref(), Some("DROP TABLE users;"));
                assert_ne!(m.up_checksum, m.down_checksum.unwrap());
            },
        },
        Case {
            label: "up file with no down file is allowed",
            files: &[("0002_add_index.up.sql", "CREATE INDEX idx ON users (id);")],
            check: |result| {
                let registry = result.expect("expected build to succeed");
                assert_eq!(registry.migrations.len(), 1);
                assert!(registry.migrations[0].down_sql.is_none());
                assert!(registry.migrations[0].down_checksum.is_none());
            },
        },
        Case {
            label: "empty file list is valid, just empty",
            files: &[],
            check: |result| {
                let registry = result.expect("expected build to succeed");
                assert!(registry.migrations.is_empty());
            },
        },
        Case {
            label: "results come back sorted by version",
            files: &[
                ("0003_third.up.sql", "-- third"),
                ("0001_first.up.sql", "-- first"),
                ("0002_second.up.sql", "-- second"),
            ],
            check: |result| {
                let registry = result.expect("expected build to succeed");
                let versions: Vec<u64> = registry.migrations.iter().map(|m| m.version).collect();
                assert_eq!(versions, vec![1, 2, 3]);
            },
        },
        Case {
            label: "two up files for the same version",
            files: &[("0004_a.up.sql", "-- a"), ("0004_b.up.sql", "-- b")],
            check: |result| {
                assert!(matches!(
                    result,
                    Err(BuildError::DuplicateUp { version: 4, .. })
                ));
            },
        },
        Case {
            label: "leading-zero padding still collides with the bare number",
            files: &[
                ("0005_seed.up.sql", "INSERT INTO t VALUES (1);"),
                ("5_seed.up.sql", "INSERT INTO t VALUES (2);"),
            ],
            check: |result| {
                assert!(matches!(
                    result,
                    Err(BuildError::DuplicateUp { version: 5, .. })
                ));
            },
        },
        Case {
            label: "two down files for the same version",
            files: &[
                ("0006_a.up.sql", "-- a"),
                ("0006_a.down.sql", "-- undo a"),
                ("0006_a.down.sql", "-- undo a again"),
            ],
            check: |result| {
                assert!(matches!(result, Err(BuildError::DuplicateDown { version: 6 })));
            },
        },
        Case {
            label: "down file with no matching up file",
            files: &[("0007_a.down.sql", "DROP TABLE a;")],
            check: |result| {
                assert!(matches!(result, Err(BuildError::MissingUp { version: 7 })));
            },
        },
        Case {
            label: "up and down disagree on the migration name",
            files: &[
                ("0008_add_a.up.sql", "-- add a"),
                ("0008_remove_a.down.sql", "-- remove a"),
            ],
            check: |result| {
                assert!(matches!(
                    result,
                    Err(BuildError::NameMismatch { version: 8, .. })
                ));
            },
        },
    ];

    for case in cases {
        // Printed eagerly so a panic inside `check` still shows which case
        // was running, since normal assertion output doesn't know the label.
        println!("case: {}", case.label);
        (case.check)(Registry::build(case.files));
    }
}

#[test]
fn registry_get_finds_by_version_regardless_of_padding() {
    let registry = Registry::build(&[
        ("0001_first.up.sql", "-- first"),
        ("0002_second.up.sql", "-- second"),
    ])
    .expect("expected build to succeed");

    let found = registry.get(2).expect("expected version 2 to be present");
    assert_eq!(found.name, "second");

    // A filename's leading zeros don't survive into the stored version, so
    // looking up the bare number must still work.
    let found = registry.get(1).expect("expected version 1 to be present");
    assert_eq!(found.name, "first");

    assert!(registry.get(3).is_none());
    assert!(registry.get(0).is_none());
}

#[test]
fn a_malformed_filename_reports_which_file_failed() {
    let result = Registry::build(&[("nope.sql", "select 1;")]);
    match result {
        Err(BuildError::Parse { file, .. }) => assert_eq!(file, "nope.sql"),
        other => panic!("expected a parse error, got {:?}", other),
    }
}
