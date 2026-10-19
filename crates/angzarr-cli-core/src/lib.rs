//! The angzarr component model, read from the `io.angzarr.v1` options in a
//! compiled descriptor set, the lint rules that check it, and a project's
//! committed settings ([`project`]: `.angzarr/config.yaml` and `lock.yaml`).
//!
//! Pure: no I/O, no argument parsing, no file system. A descriptor set comes
//! in as a [`DescriptorSet`]; results go out as data.

pub mod lint;
pub mod model;
pub mod options;
pub mod project;
pub mod run;

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

/// This crate's en-US messages.
pub const EN_US: &str = include_str!("../locales/en-US/angzarr-cli-core.ftl");

/// One of every diagnostic this crate raises, for the catalog check.
pub fn diagnostic_samples() -> Vec<Box<dyn annals_diagnostic::Diagnostic>> {
    let mut samples = lint::samples();
    samples.extend(run::samples());
    samples.extend(project::samples());
    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_has_every_code_with_exactly_its_args() {
        let problems = angzarr_fmt::check(EN_US, "angzarr-cli-core", &diagnostic_samples());
        assert!(problems.is_empty(), "{problems:#?}");
    }

    #[test]
    fn the_catalog_renders_a_selected_kind() {
        let mut catalog = angzarr_fmt::Catalog::en_us();
        catalog.add(EN_US).unwrap();
        let lint = lint::Lint::MissingField {
            component: "a.v1.Flow".into(),
            kind: "process-manager",
            field: "domain",
        };
        assert_eq!(
            catalog.render(&lint),
            "a.v1.Flow: a process manager requires (component).domain"
        );
        let remedy = lint.remedy().unwrap();
        assert_eq!(catalog.render(&remedy), "set (component).domain");
        let mark = lint::Lint::NoMarkedFields {
            message: "m".into(),
        }
        .remedy()
        .unwrap();
        assert_eq!(catalog.render(&mark), mark.to_string());
    }
}
