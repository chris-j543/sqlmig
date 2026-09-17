use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use crate::migration::Migration;
use crate::registry::Registry;

/// The migrations that still need to run to bring a database up to date with
/// a [`Registry`], in ascending version order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPlan<'a> {
    pub pending: Vec<&'a Migration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanError {
    /// A version is recorded as applied but isn't in the registry, so
    /// there's no way to tell whether it's safe to leave alone or how to
    /// roll it back.
    AppliedVersionNotInRegistry { version: u64 },
    /// Rolling back this version would need its `down.sql`, which the
    /// migration was built without.
    MissingDownSql { version: u64 },
    /// The `up_sql` on file for this version no longer matches the checksum
    /// recorded when it was applied, so someone edited a migration after it
    /// already ran.
    ChecksumMismatch {
        version: u64,
        recorded: u64,
        current: u64,
    },
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::AppliedVersionNotInRegistry { version } => write!(
                f,
                "version {version} is recorded as applied but is not in the registry"
            ),
            PlanError::MissingDownSql { version } => write!(
                f,
                "version {version} has no down.sql, so it cannot be rolled back"
            ),
            PlanError::ChecksumMismatch {
                version,
                recorded,
                current,
            } => write!(
                f,
                "version {version} was applied with checksum {recorded:x} but the file on disk now checksums to {current:x}"
            ),
        }
    }
}

impl Error for PlanError {}

/// One migration a caller's bookkeeping says has already run, paired with
/// the `up_checksum` that was recorded at the time it applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppliedMigration {
    pub version: u64,
    pub up_checksum: u64,
}

impl Registry {
    fn known_versions(&self) -> BTreeSet<u64> {
        self.migrations.iter().map(|m| m.version).collect()
    }

    /// Diffs `applied` (versions already recorded as run against a
    /// database) against this registry and returns the migrations that
    /// still need to run, in ascending version order.
    pub fn plan_apply(&self, applied: &[u64]) -> Result<ApplyPlan<'_>, PlanError> {
        let known = self.known_versions();
        let applied: BTreeSet<u64> = applied.iter().copied().collect();
        if let Some(&version) = applied.difference(&known).next() {
            return Err(PlanError::AppliedVersionNotInRegistry { version });
        }

        let pending = self
            .migrations
            .iter()
            .filter(|m| !applied.contains(&m.version))
            .collect();
        Ok(ApplyPlan { pending })
    }

    /// Diffs `applied` against this registry and returns the migrations
    /// that need to be rolled back to bring the database down to `target`,
    /// in descending version order (so callers can run them in that order
    /// without re-sorting).
    ///
    /// A migration is included if it's in `applied` and its version is
    /// greater than `target`. `target` doesn't need to be a real migration
    /// version; passing 0 rolls back everything, since 0 is reserved and no
    /// migration can ever be built with that version.
    pub fn plan_rollback(&self, applied: &[u64], target: u64) -> Result<Vec<&Migration>, PlanError> {
        let known = self.known_versions();
        let applied: BTreeSet<u64> = applied.iter().copied().collect();
        if let Some(&version) = applied.difference(&known).next() {
            return Err(PlanError::AppliedVersionNotInRegistry { version });
        }

        let mut rollback: Vec<&Migration> = self
            .migrations
            .iter()
            .filter(|m| m.version > target && applied.contains(&m.version))
            .collect();
        rollback.reverse();

        for m in &rollback {
            if m.down_sql.is_none() {
                return Err(PlanError::MissingDownSql { version: m.version });
            }
        }
        Ok(rollback)
    }

    /// Checks a caller's record of already-applied migrations against the
    /// `up_sql` this registry has on file for those versions.
    ///
    /// Returns the first version whose recorded checksum no longer matches,
    /// meaning the migration file was edited after it ran against a
    /// database. Fails with `AppliedVersionNotInRegistry` first if `applied`
    /// names a version this registry doesn't know about at all.
    pub fn check_drift(&self, applied: &[AppliedMigration]) -> Result<(), PlanError> {
        for record in applied {
            let migration = self
                .migrations
                .iter()
                .find(|m| m.version == record.version)
                .ok_or(PlanError::AppliedVersionNotInRegistry {
                    version: record.version,
                })?;
            if migration.up_checksum != record.up_checksum {
                return Err(PlanError::ChecksumMismatch {
                    version: record.version,
                    recorded: record.up_checksum,
                    current: migration.up_checksum,
                });
            }
        }
        Ok(())
    }
}
