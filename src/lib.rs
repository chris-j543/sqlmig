//! sqlmig turns a bag of SQL migration files into an ordered, validated list
//! of migrations, without opening a database connection or depending on any
//! driver.
//!
//! It only understands migration *metadata*: version numbers, names, and the
//! up/down SQL bodies that go with them. Callers are responsible for reading
//! files off disk (or wherever) and for actually running the SQL; this crate
//! just makes sure the set of files is internally consistent before you get
//! anywhere near a database.

mod migration;
mod plan;
mod registry;

pub use migration::{checksum, parse_filename, Direction, Migration, ParseError, ParsedName};
pub use plan::{AppliedMigration, ApplyPlan, PlanError};
pub use registry::{BuildError, Registry};
