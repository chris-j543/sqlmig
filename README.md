# sqlmig

A small Rust library for validating sets of SQL migration files before you
run them against a database.

Most projects keep migrations as pairs of files on disk, something like:

```
migrations/
  0001_create_users.up.sql
  0001_create_users.down.sql
  0002_add_email_index.up.sql
  0002_add_email_index.down.sql
```

The convention works fine until it doesn't: someone adds `1_seed_data.up.sql`
next to an existing `0001_create_users.up.sql` and now there are two
migrations claiming version 1. Or a down file gets renamed during a refactor
and no longer matches its up file's name. Or a down file is added for a
version whose up file was deleted. These are all easy to miss in a code
review and painful to hit at deploy time.

`sqlmig` doesn't touch a database, a filesystem, or a network connection. You
hand it a list of `(filename, content)` pairs and it either gives you back a
version-ordered, internally consistent list of migrations, or a specific
error describing what's wrong.

## Usage

`sqlmig::load_dir` reads a directory's `.sql` files into `(filename,
content)` pairs; it's the only part of this crate that touches a filesystem,
and it's entirely optional; skip it and build the pairs however you like if
your migrations don't live in a plain directory.

```rust
use sqlmig::{load_dir, Registry};

fn load_migrations(dir: &str) -> Result<Registry, Box<dyn std::error::Error>> {
    let files = load_dir(dir)?;
    let pairs: Vec<(&str, &str)> = files
        .iter()
        .map(|(name, content)| (name.as_str(), content.as_str()))
        .collect();

    Registry::build(&pairs).map_err(|e| e.into())
}
```

A filename must look like `VERSION_NAME.up.sql` or `VERSION_NAME.down.sql`,
where `VERSION` is an unsigned integer (leading zeros allowed, but not zero
itself - see below) and `NAME` is ASCII letters, digits, and underscores.
Building a registry fails if:

- two `up` files claim the same version number (including `0001` colliding
  with `1`)
- two `down` files claim the same version number
- a `down` file exists with no matching `up` file
- the `up` and `down` files for a version disagree on the migration name

Version 0 is reserved and can't be used by a migration file - `parse_filename`
rejects it with `ParseError::ZeroVersion`. Migrations are numbered from 1, and
0 is left free to mean "nothing has been applied yet," which is the sentinel
`plan_rollback` uses for "roll back everything" below.

Each resulting `Migration` carries an FNV-1a checksum of its `up` and `down`
bodies, meant for callers that record which migrations ran against a
database and want to detect a migration file changing after it was applied.

## Planning

Once you have a `Registry`, `plan_apply` and `plan_rollback` diff it against
whatever list of versions your database says it has already applied.
Neither one touches a database; you supply the applied versions (however
you tracked them) and get back a plan to execute yourself:

```rust
// versions your own bookkeeping table says have already run
let applied = [1, 2];

let plan = registry.plan_apply(&applied)?;
for migration in plan.pending {
    // run migration.up_sql, then record migration.version as applied
}

// roll everything back past version 1
let rollback = registry.plan_rollback(&applied, 1)?;
for migration in rollback {
    // run migration.down_sql, then remove migration.version from applied
}
```

Both fail with `PlanError::AppliedVersionNotInRegistry` if `applied`
contains a version that isn't in the registry - that means a migration ran
at some point but its files are gone now, which is worth surfacing rather
than silently ignoring. `plan_rollback` also fails with
`PlanError::MissingDownSql` if it would need to roll back a migration that
was built without a `down.sql`.

If your own bookkeeping also records the `up_checksum` a migration had when
it ran, `check_drift` will catch a migration file being edited after the
fact - a down file changing scope, someone "fixing" an up file in place
instead of writing a new migration, that kind of thing:

```rust
use sqlmig::AppliedMigration;

let applied = [
    AppliedMigration { version: 1, up_checksum: 0x1234 /* from your table */ },
];
registry.check_drift(&applied)?; // Err(PlanError::ChecksumMismatch { .. }) if version 1's up.sql changed
```

## What this crate does not do

It does not open a database connection and does not execute SQL. Reading
migration files off disk and actually running them against a database are
both left to the caller.

## License

MIT, see [LICENSE](LICENSE).
