use std::error::Error;
use std::fmt;

/// Which half of a migration pair a file represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

/// The metadata pulled out of a migration filename, e.g.
/// `0001_create_users.up.sql` -> version 1, name "create_users", Up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedName {
    pub version: u64,
    pub name: String,
    pub direction: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// The filename doesn't split into exactly `stem.direction.sql`.
    InvalidFormat,
    /// The extension segment isn't `sql`.
    InvalidExtension,
    /// The direction segment isn't `up` or `down` (case sensitive).
    InvalidDirection,
    /// The stem has no `_` separating a version from a name.
    MissingName,
    /// The version segment is empty, non-numeric, or too large for a u64.
    InvalidVersion,
    /// The version segment is literally zero. Zero is reserved to mean "no
    /// migrations applied" (see `Registry::plan_rollback`), so no migration
    /// file may claim it.
    ZeroVersion,
    /// The name segment is empty or contains characters other than
    /// ASCII letters, digits, and underscores.
    InvalidName,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            ParseError::InvalidFormat => "filename must look like VERSION_NAME.up|down.sql",
            ParseError::InvalidExtension => "filename must end in .sql",
            ParseError::InvalidDirection => "direction segment must be exactly \"up\" or \"down\"",
            ParseError::MissingName => "filename is missing a NAME segment after the version",
            ParseError::InvalidVersion => "version segment must be a plain, in-range integer",
            ParseError::ZeroVersion => "version 0 is reserved and cannot be used by a migration",
            ParseError::InvalidName => "name segment must be ASCII letters, digits, or underscores",
        };
        f.write_str(msg)
    }
}

impl Error for ParseError {}

/// Splits a migration filename into version, name, and direction.
///
/// Expects the shape `VERSION_NAME.up.sql` or `VERSION_NAME.down.sql`. The
/// version may have leading zeros (`0001` and `1` parse to the same value,
/// which the registry treats as a collision if both appear).
///
/// Version 0 is rejected outright. It's reserved to mean "nothing applied
/// yet" - `Registry::plan_rollback` takes 0 as a target to mean "roll back
/// every applied migration" - so a real migration can't claim it without
/// making that sentinel ambiguous.
pub fn parse_filename(filename: &str) -> Result<ParsedName, ParseError> {
    let parts: Vec<&str> = filename.rsplitn(3, '.').collect();
    if parts.len() != 3 {
        return Err(ParseError::InvalidFormat);
    }
    let ext = parts[0];
    let direction_str = parts[1];
    let stem = parts[2];

    if ext != "sql" {
        return Err(ParseError::InvalidExtension);
    }

    let direction = match direction_str {
        "up" => Direction::Up,
        "down" => Direction::Down,
        _ => return Err(ParseError::InvalidDirection),
    };

    let (version_str, name) = stem.split_once('_').ok_or(ParseError::MissingName)?;

    if version_str.is_empty() || !version_str.chars().all(|c| c.is_ascii_digit()) {
        return Err(ParseError::InvalidVersion);
    }
    let version: u64 = version_str.parse().map_err(|_| ParseError::InvalidVersion)?;
    if version == 0 {
        return Err(ParseError::ZeroVersion);
    }

    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(ParseError::InvalidName);
    }

    Ok(ParsedName {
        version,
        name: name.to_string(),
        direction,
    })
}

/// FNV-1a over the raw bytes of a migration body. Not cryptographic; it only
/// needs to catch "this file's content changed since it was recorded as applied".
pub fn checksum(content: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in content.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// A single migration: its identity plus the SQL that applies and (optionally)
/// reverts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    pub version: u64,
    pub name: String,
    pub up_sql: String,
    pub down_sql: Option<String>,
    pub up_checksum: u64,
    pub down_checksum: Option<u64>,
}
