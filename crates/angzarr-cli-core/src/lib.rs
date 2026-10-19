//! The angzarr component model, read from the `io.angzarr.v1` options in a
//! compiled descriptor set, and the lint rules that check it.
//!
//! Pure: no I/O, no argument parsing, no file system. A descriptor set comes
//! in as a [`DescriptorSet`]; results go out as data.

pub mod options;

#[cfg(any(test, feature = "test-support"))]
pub mod fixture;

use std::collections::BTreeSet;

use prost_reflect::DescriptorPool;

/// A compiled descriptor set as the core reads it.
#[derive(Clone, Debug)]
pub struct DescriptorSet {
    /// Every file, imports included.
    pub pool: DescriptorPool,
    /// The files generated in this run (not present only as imports), by
    /// name. A plain `FileDescriptorSet` marks none as imports, so all of
    /// its files are generated.
    pub generated: BTreeSet<String>,
}
