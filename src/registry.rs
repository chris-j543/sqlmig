use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::migration::{checksum, parse_filename, Direction, Migration, ParseError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    /// A filename didn't parse; `file` is the name that failed.
    Parse { file: String, source: ParseError },
    /// Two `up` files claimed the same version number.
    DuplicateUp {
        version: u64,
        first_name: String,
        second_name: String,
    },
    /// Two `down` files claimed the same version number.
    DuplicateDown { version: u64 },
    /// A `down` file exists with no matching `up` file for that version.
    MissingUp { version: u64 },
    /// The `up` and `down` files for a version disagree on the migration name.
    NameMismatch {
        version: u64,
        up_name: String,
        down_name: String,
    },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::Parse { file, source } => write!(f, "{file}: {source}"),
            BuildError::DuplicateUp {
                version,
                first_name,
                second_name,
            } => write!(
                f,
                "version {version} has two up files: \"{first_name}\" and \"{second_name}\""
            ),
            BuildError::DuplicateDown { version } => {
                write!(f, "version {version} has two down files")
            }
            BuildError::MissingUp { version } => {
                write!(f, "version {version} has a down file but no up file")
            }
            BuildError::NameMismatch {
                version,
                up_name,
                down_name,
            } => write!(
                f,
                "version {version} names disagree: up is \"{up_name}\", down is \"{down_name}\""
            ),
        }
    }
}

impl Error for BuildError {}

/// A validated, version-ordered set of migrations.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Registry {
    pub migrations: Vec<Migration>,
}

struct Entry {
    up: Option<(String, String)>,
    down: Option<(String, String)>,
}

impl Registry {
    /// Builds a registry from a bag of `(filename, content)` pairs.
    ///
    /// Files are matched into up/down pairs by version number, not by input
    /// order, so callers can pass files in whatever order a directory listing
    /// returns them.
    pub fn build(files: &[(&str, &str)]) -> Result<Registry, BuildError> {
        let mut entries: BTreeMap<u64, Entry> = BTreeMap::new();

        for (filename, content) in files {
            let parsed = parse_filename(filename).map_err(|source| BuildError::Parse {
                file: (*filename).to_string(),
                source,
            })?;
            let entry = entries.entry(parsed.version).or_insert(Entry {
                up: None,
                down: None,
            });
            match parsed.direction {
                Direction::Up => {
                    if let Some((existing_name, _)) = &entry.up {
                        return Err(BuildError::DuplicateUp {
                            version: parsed.version,
                            first_name: existing_name.clone(),
                            second_name: parsed.name,
                        });
                    }
                    entry.up = Some((parsed.name, (*content).to_string()));
                }
                Direction::Down => {
                    if entry.down.is_some() {
                        return Err(BuildError::DuplicateDown {
                            version: parsed.version,
                        });
                    }
                    entry.down = Some((parsed.name, (*content).to_string()));
                }
            }
        }

        let mut migrations = Vec::with_capacity(entries.len());
        for (version, entry) in entries {
            let (up_name, up_sql) = entry.up.ok_or(BuildError::MissingUp { version })?;
            let down_sql = match entry.down {
                Some((down_name, down_sql)) if down_name == up_name => Some(down_sql),
                Some((down_name, _)) => {
                    return Err(BuildError::NameMismatch {
                        version,
                        up_name,
                        down_name,
                    })
                }
                None => None,
            };
            let up_checksum = checksum(&up_sql);
            let down_checksum = down_sql.as_deref().map(checksum);
            migrations.push(Migration {
                version,
                name: up_name,
                up_sql,
                down_sql,
                up_checksum,
                down_checksum,
            });
        }

        Ok(Registry { migrations })
    }
}
