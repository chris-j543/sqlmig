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

```rust
use std::fs;
use sqlmig::{Registry, BuildError};

fn load_migrations(dir: &str) -> Result<Registry, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    let mut contents = Vec::new();

    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        contents.push(fs::read_to_string(&path)?);
        files.push(path.file_name().unwrap().to_string_lossy().into_owned());
    }

    let pairs: Vec<(&str, &str)> = files
        .iter()
        .map(String::as_str)
        .zip(contents.iter().map(String::as_str))
        .collect();

    Registry::build(&pairs).map_err(|e| e.into())
}
```

A filename must look like `VERSION_NAME.up.sql` or `VERSION_NAME.down.sql`,
where `VERSION` is an unsigned integer (leading zeros allowed) and `NAME` is
ASCII letters, digits, and underscores. Building a registry fails if:

- two `up` files claim the same version number (including `0001` colliding
  with `1`)
- two `down` files claim the same version number
- a `down` file exists with no matching `up` file
- the `up` and `down` files for a version disagree on the migration name

Each resulting `Migration` carries an FNV-1a checksum of its `up` and `down`
bodies, meant for callers that record which migrations ran against a
database and want to detect a migration file changing after it was applied.

## What this crate does not do

It does not open a database connection, does not execute SQL, and does not
decide which migrations still need to run against a particular database.
That's deliberately out of scope for now; see the roadmap below.

## License

MIT, see [LICENSE](LICENSE).
