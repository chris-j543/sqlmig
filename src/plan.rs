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
        }
    }
}

impl Error for PlanError {}

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
    /// version; passing 0 rolls back everything.
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
}
