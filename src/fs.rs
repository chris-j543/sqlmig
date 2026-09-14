use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Reads every `.sql` file directly inside `dir` into a `(filename, content)`
/// pair, ready to hand to [`crate::Registry::build`].
///
/// Subdirectories and any file whose name doesn't end in `.sql` are skipped
/// rather than treated as an error, so a stray `.gitkeep` or editor swap file
/// left in a migrations directory doesn't need to be cleaned out by hand.
/// This function does not otherwise validate filenames; a `.sql` file that
/// doesn't match `VERSION_NAME.up|down.sql` is still returned here and will
/// fail when `Registry::build` parses it.
///
/// The result is sorted by filename so that building a registry from it is
/// deterministic regardless of the order the OS happens to list directory
/// entries in.
pub fn load_dir(dir: impl AsRef<Path>) -> Result<Vec<(String, String)>, LoadError> {
    let dir = dir.as_ref();
    let mut pairs = Vec::new();

    let entries = fs::read_dir(dir).map_err(|source| LoadError::new(dir, source))?;

    for entry in entries {
        let entry = entry.map_err(|source| LoadError::new(dir, source))?;
        let path = entry.path();

        let is_file = entry
            .file_type()
            .map_err(|source| LoadError::new(&path, source))?
            .is_file();
        if !is_file {
            continue;
        }

        let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !filename.ends_with(".sql") {
            continue;
        }

        let content = fs::read_to_string(&path).map_err(|source| LoadError::new(&path, source))?;
        pairs.push((filename.to_string(), content));
    }

    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(pairs)
}

/// An I/O failure while reading a migrations directory, with the path that
/// caused it attached.
#[derive(Debug)]
pub struct LoadError {
    path: PathBuf,
    source: io::Error,
}

impl LoadError {
    fn new(path: impl Into<PathBuf>, source: io::Error) -> Self {
        LoadError {
            path: path.into(),
            source,
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.source)
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn unique_temp_dir() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "sqlmig-fs-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            n
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn loads_sql_files_sorted_and_skips_the_rest() {
        let dir = unique_temp_dir();

        fs::write(dir.join("0002_add_email_index.up.sql"), "create index").unwrap();
        fs::write(dir.join("0001_create_users.up.sql"), "create table").unwrap();
        fs::write(dir.join(".gitkeep"), "").unwrap();
        fs::create_dir_all(dir.join("nested")).unwrap();
        fs::write(dir.join("nested/0003_ignored.up.sql"), "ignored").unwrap();

        let pairs = load_dir(&dir).unwrap();

        assert_eq!(
            pairs,
            vec![
                (
                    "0001_create_users.up.sql".to_string(),
                    "create table".to_string()
                ),
                (
                    "0002_add_email_index.up.sql".to_string(),
                    "create index".to_string()
                ),
            ]
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_directory_is_an_error() {
        let dir = std::env::temp_dir().join("sqlmig-fs-test-does-not-exist");
        let err = load_dir(&dir).unwrap_err();
        assert_eq!(err.path, dir);
    }
}
