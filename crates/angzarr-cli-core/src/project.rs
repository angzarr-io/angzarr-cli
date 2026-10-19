//! An angzarr project's committed settings: the `.angzarr/` directory at the
//! project root holds what makes every checkout generate the same thing.
//!
//! - `config.yaml` ([`Config`]): the language targets, where the protos
//!   are, where each target's generated wiring goes, and output defaults.
//! - `lock.yaml` ([`Lock`]): the angzarr-project spec commit the project
//!   builds against, and the client API version each target emits for.
//! - `cache/` is never committed: the `.gitignore` written beside the two
//!   files ignores it.
//!
//! Nothing about a run is kept here. This module is the data and its
//! validation; reading and writing the files is an adapter's.

use std::collections::BTreeMap;
use std::fmt;

use annals_diagnostic::{Arg, Diagnostic, Report};
use serde::Serialize;

/// The project directory, at the project root.
pub const DIR: &str = ".angzarr";
/// The settings file, in [`DIR`].
pub const CONFIG_FILE: &str = "config.yaml";
/// The lock file, in [`DIR`].
pub const LOCK_FILE: &str = "lock.yaml";
/// The ignore file, in [`DIR`].
pub const GITIGNORE_FILE: &str = ".gitignore";
/// What the ignore file holds: the cache is never committed.
pub const GITIGNORE: &str = "cache/\n";
/// The `version` `config.yaml` is read at.
pub const CONFIG_VERSION: u32 = 1;
/// The `version` `lock.yaml` is read at.
pub const LOCK_VERSION: u32 = 1;

/// The files a new project directory holds, relative to the project root.
pub fn files() -> Vec<String> {
    [CONFIG_FILE, LOCK_FILE, GITIGNORE_FILE]
        .iter()
        .map(|f| format!("{DIR}/{f}"))
        .collect()
}

/// A language angzarr generates wiring for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Rust,
    Python,
}

impl Target {
    pub const ALL: [Target; 2] = [Target::Rust, Target::Python];

    /// The name a user writes.
    pub fn token(self) -> &'static str {
        match self {
            Target::Rust => "rust",
            Target::Python => "python",
        }
    }

    /// The target `token` names; `source` (a file or a flag) is named in the
    /// failure.
    pub fn parse(source: &str, token: &str) -> Result<Target, ProjectError> {
        Target::ALL
            .into_iter()
            .find(|t| t.token() == token)
            .ok_or_else(|| ProjectError::UnknownTarget {
                source: source.to_string(),
                target: token.to_string(),
            })
    }

    /// Where a new project puts this target's generated wiring.
    pub fn default_out(self) -> String {
        format!("gen/{}", self.token())
    }
}

/// The format a command writes in when `--format` is not given and stdout
/// is not a terminal. The same names as `--format`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NonTtyFormat {
    Json,
    Yaml,
    Toml,
    Text,
    Markdown,
}

impl NonTtyFormat {
    pub const ALL: [NonTtyFormat; 5] = [
        NonTtyFormat::Json,
        NonTtyFormat::Yaml,
        NonTtyFormat::Toml,
        NonTtyFormat::Text,
        NonTtyFormat::Markdown,
    ];

    /// The name a user writes.
    pub fn token(self) -> &'static str {
        match self {
            NonTtyFormat::Json => "json",
            NonTtyFormat::Yaml => "yaml",
            NonTtyFormat::Toml => "toml",
            NonTtyFormat::Text => "text",
            NonTtyFormat::Markdown => "markdown",
        }
    }

    /// The format `token` names; `source` is named in the failure.
    pub fn parse(source: &str, token: &str) -> Result<NonTtyFormat, ProjectError> {
        NonTtyFormat::ALL
            .into_iter()
            .find(|f| f.token() == token)
            .ok_or_else(|| ProjectError::BadFormat {
                source: source.to_string(),
                value: token.to_string(),
            })
    }
}

/// A full commit id: 40 hex digits (SHA-1) or 64 (SHA-256), kept in lower
/// case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SpecCommit(String);

impl SpecCommit {
    /// The commit `text` names in full; `source` is named in the failure.
    pub fn parse(source: &str, text: &str) -> Result<SpecCommit, ProjectError> {
        let full = matches!(text.len(), 40 | 64) && text.bytes().all(|b| b.is_ascii_hexdigit());
        if full {
            Ok(SpecCommit(text.to_ascii_lowercase()))
        } else {
            Err(ProjectError::BadCommit {
                source: source.to_string(),
                value: text.to_string(),
            })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `config.yaml`: what the project generates, from where, to where.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Config {
    /// The directory the project's protos are in, relative to the project
    /// root.
    pub protos: String,
    /// The enabled targets.
    pub targets: BTreeMap<Target, TargetConfig>,
    pub output: OutputConfig,
}

impl Config {
    /// A new project's config: protos in `proto/`, each target generating
    /// into `gen/<target>`.
    pub fn new(targets: impl IntoIterator<Item = Target>) -> Config {
        Config {
            protos: "proto".to_string(),
            targets: targets
                .into_iter()
                .map(|t| {
                    (
                        t,
                        TargetConfig {
                            out: t.default_out(),
                        },
                    )
                })
                .collect(),
            output: OutputConfig::default(),
        }
    }
}

/// One target's settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TargetConfig {
    /// The directory its generated wiring goes in, relative to the project
    /// root.
    pub out: String,
}

/// Output defaults.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct OutputConfig {
    /// The format off a terminal when `--format` is not given; json when
    /// unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub non_tty_format: Option<NonTtyFormat>,
}

/// `lock.yaml`: what the project builds against.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Lock {
    pub spec: SpecLock,
    /// The client API version each target's generated wiring is for.
    pub targets: BTreeMap<Target, TargetLock>,
}

impl Lock {
    /// A new project's lock: the spec commit, and no target locked yet.
    pub fn new(commit: SpecCommit) -> Lock {
        Lock {
            spec: SpecLock { commit },
            targets: BTreeMap::new(),
        }
    }
}

/// The angzarr-project spec the project builds against.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpecLock {
    pub commit: SpecCommit,
}

/// One target's locked client API.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TargetLock {
    pub client_api: String,
}

/// What a value is expected to be, where it is something else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Mapping,
    Scalar,
}

impl Shape {
    pub fn token(self) -> &'static str {
        match self {
            Shape::Mapping => "mapping",
            Shape::Scalar => "scalar",
        }
    }
}

/// Why project settings are not valid. `source` is where a value came
/// from: a file (`.angzarr/config.yaml`) or a flag (`--target`). A `key`
/// is a dotted path from the file's root (`output.non_tty_format`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectError {
    UnknownTarget {
        source: String,
        target: String,
    },
    BadFormat {
        source: String,
        value: String,
    },
    BadCommit {
        source: String,
        value: String,
    },
    UnknownKey {
        file: String,
        key: String,
    },
    MissingKey {
        file: String,
        key: String,
    },
    WrongShape {
        file: String,
        key: String,
        shape: Shape,
    },
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectError::UnknownTarget { source, target } => {
                write!(
                    f,
                    "{source}: \"{target}\" is not a target angzarr generates for"
                )
            }
            ProjectError::BadFormat { source, value } => {
                write!(f, "{source}: \"{value}\" is not an output format")
            }
            ProjectError::BadCommit { source, value } => {
                write!(f, "{source}: \"{value}\" is not a full commit id")
            }
            ProjectError::UnknownKey { file, key } => write!(f, "{file}: unknown key {key}"),
            ProjectError::MissingKey { file, key } => write!(f, "{file}: {key} is required"),
            ProjectError::WrongShape { file, key, shape } => match shape {
                Shape::Mapping => write!(f, "{file}: {key} must be a mapping"),
                Shape::Scalar => write!(f, "{file}: {key} must be a single value"),
            },
        }
    }
}

impl std::error::Error for ProjectError {}

impl Diagnostic for ProjectError {
    fn code(&self) -> &'static str {
        match self {
            ProjectError::UnknownTarget { .. } => "angzarr-cli-core-project-unknown-target",
            ProjectError::BadFormat { .. } => "angzarr-cli-core-project-bad-format",
            ProjectError::BadCommit { .. } => "angzarr-cli-core-project-bad-commit",
            ProjectError::UnknownKey { .. } => "angzarr-cli-core-project-unknown-key",
            ProjectError::MissingKey { .. } => "angzarr-cli-core-project-missing-key",
            ProjectError::WrongShape { .. } => "angzarr-cli-core-project-wrong-shape",
        }
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        let s = |s: &String| Arg::from(s.as_str());
        match self {
            ProjectError::UnknownTarget { source, target } => {
                vec![("source", s(source)), ("target", s(target))]
            }
            ProjectError::BadFormat { source, value }
            | ProjectError::BadCommit { source, value } => {
                vec![("source", s(source)), ("value", s(value))]
            }
            ProjectError::UnknownKey { file, key } | ProjectError::MissingKey { file, key } => {
                vec![("file", s(file)), ("key", s(key))]
            }
            ProjectError::WrongShape { file, key, shape } => vec![
                ("file", s(file)),
                ("key", s(key)),
                ("shape", shape.token().into()),
            ],
        }
    }
}

impl ProjectError {
    /// What to do about it, when there is something to say.
    pub fn remedy(&self) -> Option<Report> {
        let r = Report::new;
        Some(match self {
            ProjectError::UnknownTarget { .. } => {
                r("angzarr-cli-core-fix-target", "use rust or python")
            }
            ProjectError::BadFormat { .. } => r(
                "angzarr-cli-core-fix-format",
                "use json, yaml, toml, text or markdown",
            ),
            ProjectError::BadCommit { .. } => r(
                "angzarr-cli-core-fix-commit",
                "give the angzarr-project commit in full, as git rev-parse prints it: 40 hex digits, or 64 for a SHA-256 repository",
            ),
            ProjectError::UnknownKey { key, .. } => Report::new(
                "angzarr-cli-core-fix-unknown-key",
                format!("remove {key}, or correct its spelling"),
            )
            .arg("key", key.as_str()),
            ProjectError::MissingKey { key, .. } => {
                Report::new("angzarr-cli-core-fix-missing-key", format!("set {key}"))
                    .arg("key", key.as_str())
            }
            ProjectError::WrongShape { .. } => return None,
        })
    }
}

/// One of every diagnostic this module raises, remedies included.
pub fn samples() -> Vec<Box<dyn Diagnostic>> {
    let s = || "x".to_string();
    let errors = vec![
        ProjectError::UnknownTarget {
            source: s(),
            target: s(),
        },
        ProjectError::BadFormat {
            source: s(),
            value: s(),
        },
        ProjectError::BadCommit {
            source: s(),
            value: s(),
        },
        ProjectError::UnknownKey {
            file: s(),
            key: s(),
        },
        ProjectError::MissingKey {
            file: s(),
            key: s(),
        },
        ProjectError::WrongShape {
            file: s(),
            key: s(),
            shape: Shape::Mapping,
        },
    ];
    let mut out: Vec<Box<dyn Diagnostic>> = Vec::new();
    for e in errors {
        if let Some(remedy) = e.remedy() {
            out.push(Box::new(remedy));
        }
        out.push(Box::new(e));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA1: &str = "b1f00d2710eb7bee60308f6df520893d782012a6";

    #[test]
    fn a_target_parses_from_its_token_and_nothing_else() {
        assert_eq!(Target::parse("f", "rust"), Ok(Target::Rust));
        assert_eq!(Target::parse("f", "python"), Ok(Target::Python));
        let err = Target::parse("--target", "Rust").unwrap_err();
        assert_eq!(
            err,
            ProjectError::UnknownTarget {
                source: "--target".into(),
                target: "Rust".into()
            }
        );
        assert_eq!(err.code(), "angzarr-cli-core-project-unknown-target");
        assert_eq!(err.remedy().unwrap().code(), "angzarr-cli-core-fix-target");
    }

    #[test]
    fn a_non_tty_format_is_one_of_the_five_formats() {
        let names: Vec<_> = NonTtyFormat::ALL.iter().map(|f| f.token()).collect();
        assert_eq!(names, ["json", "yaml", "toml", "text", "markdown"]);
        for f in NonTtyFormat::ALL {
            assert_eq!(NonTtyFormat::parse("f", f.token()), Ok(f));
        }
        let err = NonTtyFormat::parse("f", "xml").unwrap_err();
        assert_eq!(err.code(), "angzarr-cli-core-project-bad-format");
        assert_eq!(err.remedy().unwrap().code(), "angzarr-cli-core-fix-format");
    }

    #[test]
    fn a_spec_commit_is_a_full_hex_sha_kept_in_lower_case() {
        assert_eq!(SpecCommit::parse("f", SHA1).unwrap().as_str(), SHA1);
        let upper = SHA1.to_ascii_uppercase();
        assert_eq!(SpecCommit::parse("f", &upper).unwrap().as_str(), SHA1);
        let sha256 = "a".repeat(64);
        assert_eq!(SpecCommit::parse("f", &sha256).unwrap().as_str(), sha256);
        for bad in [
            "",
            "b1f00d2",
            &SHA1[..39],
            &format!("{SHA1}0"),
            &"g".repeat(40),
        ] {
            let err = SpecCommit::parse("--spec-commit", bad).unwrap_err();
            assert_eq!(err.code(), "angzarr-cli-core-project-bad-commit", "{bad}");
            assert_eq!(err.remedy().unwrap().code(), "angzarr-cli-core-fix-commit");
        }
    }

    #[test]
    fn a_new_config_generates_each_target_into_gen() {
        let config = Config::new([Target::Python, Target::Rust]);
        assert_eq!(config.protos, "proto");
        let outs: Vec<_> = config
            .targets
            .iter()
            .map(|(t, c)| (t.token(), c.out.as_str()))
            .collect();
        assert_eq!(outs, [("rust", "gen/rust"), ("python", "gen/python")]);
        assert_eq!(config.output.non_tty_format, None);
    }

    #[test]
    fn a_new_lock_names_the_commit_and_no_target() {
        let lock = Lock::new(SpecCommit::parse("f", SHA1).unwrap());
        assert_eq!(lock.spec.commit.as_str(), SHA1);
        assert!(lock.targets.is_empty());
    }

    #[test]
    fn the_files_live_in_the_project_directory() {
        assert_eq!(
            files(),
            [
                ".angzarr/config.yaml",
                ".angzarr/lock.yaml",
                ".angzarr/.gitignore"
            ]
        );
        assert_eq!(GITIGNORE, "cache/\n");
    }

    #[test]
    fn key_errors_name_the_file_and_the_key_and_say_what_to_do() {
        let unknown = ProjectError::UnknownKey {
            file: ".angzarr/config.yaml".into(),
            key: "output.colour".into(),
        };
        assert_eq!(unknown.code(), "angzarr-cli-core-project-unknown-key");
        assert_eq!(
            unknown.to_string(),
            ".angzarr/config.yaml: unknown key output.colour"
        );
        assert_eq!(
            unknown.remedy().unwrap().to_string(),
            "remove output.colour, or correct its spelling"
        );
        let missing = ProjectError::MissingKey {
            file: "l".into(),
            key: "spec.commit".into(),
        };
        assert_eq!(missing.remedy().unwrap().to_string(), "set spec.commit");
        let shape = ProjectError::WrongShape {
            file: "c".into(),
            key: "targets".into(),
            shape: Shape::Mapping,
        };
        assert_eq!(shape.to_string(), "c: targets must be a mapping");
        assert!(shape.remedy().is_none());
    }
}
