//! The [`Project`] port's adapters, and the YAML both share.
//!
//! `config.yaml` and `lock.yaml` are annals-yaml versioned documents: a
//! root `version: N` read through [`annals_yaml::versioned`]. Both are at
//! version 1. Changing either's schema means bumping its version in
//! `angzarr_cli_core::project`, registering the step from the old version
//! in [`config_loader`] or [`lock_loader`], and keeping a fixture at the old
//! version with its expected current-version output (the fixtures under
//! `fixtures/project/`, checked by `Loader::assert_fixtures`).
//!
//! A key the schema does not know is refused, never ignored.
//!
//! - [`DirProject`]: `.angzarr/` under a directory on disk.
//! - [`MemoryProject`]: the same files held in memory, for tests.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use angzarr_cli_app::{AppError, Project, ProjectInvalid};
use angzarr_cli_core::project::{
    CONFIG_FILE, CONFIG_VERSION, Config, DIR, GITIGNORE, GITIGNORE_FILE, LOCK_FILE, LOCK_VERSION,
    Lock, NonTtyFormat, OutputConfig, ProjectError, Shape, SpecCommit, SpecLock, Target,
    TargetConfig, TargetLock,
};
use angzarr_fmt::{Failure, io_kind};
use annals_diagnostic::{Arg, Diagnostic, Report};
use annals_yaml::Value;
use annals_yaml::versioned::{LoadError, LoadProblem, Loader, Migrations};

/// The kind `config.yaml` is named by in annals-yaml's messages.
const CONFIG_KIND: &str = "angzarr config";
/// The kind `lock.yaml` is named by in annals-yaml's messages.
const LOCK_KIND: &str = "angzarr lock";

/// Reads `config.yaml` at any supported version.
pub fn config_loader() -> Loader {
    Migrations::new(CONFIG_KIND, CONFIG_VERSION)
        .build()
        .expect("the config migration chain is whole; its tests check it")
}

/// Reads `lock.yaml` at any supported version.
pub fn lock_loader() -> Loader {
    Migrations::new(LOCK_KIND, LOCK_VERSION)
        .build()
        .expect("the lock migration chain is whole; its tests check it")
}

/// Why a project file could not be read or written. `file` is its path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectFileError {
    /// `kind` is the I/O error kind as a kebab-case token (`not-found`).
    Read {
        file: String,
        kind: &'static str,
    },
    Write {
        file: String,
        kind: &'static str,
    },
    /// The file's `version` is newer than this build reads: annals-yaml's
    /// `too-new` is the cause.
    TooNew {
        file: String,
        cause: LoadProblem,
    },
    /// Not YAML this build reads, or its `version` is not one.
    Load {
        file: String,
        cause: LoadProblem,
    },
}

impl ProjectFileError {
    fn loading(error: LoadError) -> ProjectFileError {
        let file = error.source.unwrap_or_default();
        match error.problem {
            cause @ LoadProblem::TooNew { .. } => ProjectFileError::TooNew { file, cause },
            cause => ProjectFileError::Load { file, cause },
        }
    }
}

impl fmt::Display for ProjectFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectFileError::Read { file, kind } => write!(f, "{file} could not be read: {kind}"),
            ProjectFileError::Write { file, kind } => {
                write!(f, "{file} could not be written: {kind}")
            }
            ProjectFileError::TooNew { file, cause } => {
                write!(f, "{file} was written by a newer angzarr: {cause}")
            }
            ProjectFileError::Load { file, cause } => write!(f, "{file} is not valid: {cause}"),
        }
    }
}

impl std::error::Error for ProjectFileError {}

impl Diagnostic for ProjectFileError {
    fn code(&self) -> &'static str {
        match self {
            ProjectFileError::Read { .. } => "angzarr-cli-adapters-project-read",
            ProjectFileError::Write { .. } => "angzarr-cli-adapters-project-write",
            ProjectFileError::TooNew { .. } => "angzarr-cli-adapters-project-too-new",
            ProjectFileError::Load { .. } => "angzarr-cli-adapters-project-load",
        }
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        match self {
            ProjectFileError::Read { file, kind } | ProjectFileError::Write { file, kind } => {
                vec![("file", file.as_str().into()), ("kind", (*kind).into())]
            }
            ProjectFileError::TooNew { file, cause } | ProjectFileError::Load { file, cause } => {
                vec![
                    ("file", file.as_str().into()),
                    ("cause", Arg::nested(cause)),
                ]
            }
        }
    }
}

impl Failure for ProjectFileError {
    fn remedy(&self) -> Option<Report> {
        match self {
            ProjectFileError::TooNew { .. } => Some(Report::new(
                "angzarr-cli-adapters-fix-upgrade",
                "upgrade angzarr to a build that reads this version",
            )),
            _ => None,
        }
    }
}

/// One of every diagnostic this module raises, remedies included.
pub(crate) fn samples() -> Vec<Box<dyn Diagnostic>> {
    let s = || "x".to_string();
    let too_new = ProjectFileError::TooNew {
        file: s(),
        cause: LoadProblem::TooNew {
            found: 2,
            current: 1,
        },
    };
    let remedy = too_new.remedy().expect("too new has a remedy");
    vec![
        Box::new(ProjectFileError::Read {
            file: s(),
            kind: "not-found",
        }),
        Box::new(ProjectFileError::Write {
            file: s(),
            kind: "not-found",
        }),
        Box::new(too_new),
        Box::new(remedy),
        Box::new(ProjectFileError::Load {
            file: s(),
            cause: LoadProblem::NotAMapping,
        }),
    ]
}

/// Walks a document's mappings, refusing unknown keys. `key` arguments are
/// dotted paths from the root.
struct Reader<'a> {
    file: &'a str,
}

/// A mapping's entries.
type Entries<'v> = &'v [(String, Value)];

fn join(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{parent}.{key}")
    }
}

impl Reader<'_> {
    fn shape(&self, key: &str, shape: Shape) -> ProjectError {
        ProjectError::WrongShape {
            file: self.file.to_string(),
            key: key.to_string(),
            shape,
        }
    }

    /// The mapping at `key`, whose keys must each be in `known`. A null is
    /// an empty mapping.
    fn map<'v>(
        &self,
        v: &'v Value,
        key: &str,
        known: &[&str],
    ) -> Result<Entries<'v>, ProjectError> {
        let entries = match v {
            Value::Null => &[][..],
            Value::Map(entries) => entries.as_slice(),
            _ => return Err(self.shape(key, Shape::Mapping)),
        };
        if let Some((k, _)) = entries.iter().find(|(k, _)| !known.contains(&k.as_str())) {
            return Err(ProjectError::UnknownKey {
                file: self.file.to_string(),
                key: join(key, k),
            });
        }
        Ok(entries)
    }

    /// The scalar at `key`.
    fn scalar<'v>(&self, v: &'v Value, key: &str) -> Result<&'v str, ProjectError> {
        v.as_str().ok_or_else(|| self.shape(key, Shape::Scalar))
    }

    /// The value of `name` in `entries` (the mapping at `parent`), absent
    /// when missing or null.
    fn optional<'v>(&self, entries: Entries<'v>, name: &str) -> Option<&'v Value> {
        entries
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
            .filter(|v| **v != Value::Null)
    }

    fn required<'v>(
        &self,
        entries: Entries<'v>,
        parent: &str,
        name: &str,
    ) -> Result<&'v Value, ProjectError> {
        self.optional(entries, name)
            .ok_or_else(|| ProjectError::MissingKey {
                file: self.file.to_string(),
                key: join(parent, name),
            })
    }

    /// A `targets` mapping, each entry decoded by `entry`.
    fn targets<T>(
        &self,
        root: Entries<'_>,
        entry: impl Fn(&Self, &Value, &str) -> Result<T, ProjectError>,
    ) -> Result<BTreeMap<Target, T>, ProjectError> {
        let Some(v) = self.optional(root, "targets") else {
            return Ok(BTreeMap::new());
        };
        let entries = match v {
            Value::Map(entries) => entries,
            _ => return Err(self.shape("targets", Shape::Mapping)),
        };
        let mut out = BTreeMap::new();
        for (name, value) in entries {
            let target = Target::parse(self.file, name)?;
            out.insert(target, entry(self, value, &join("targets", name))?);
        }
        Ok(out)
    }
}

fn decode_config(file: &str, doc: &Value) -> Result<Config, ProjectError> {
    let r = Reader { file };
    let root = r.map(doc, "", &["version", "protos", "targets", "output"])?;
    let protos = r
        .scalar(r.required(root, "", "protos")?, "protos")?
        .to_string();
    let targets = r.targets(root, |r, v, key| {
        let entries = r.map(v, key, &["out"])?;
        let out = r.scalar(r.required(entries, key, "out")?, &join(key, "out"))?;
        Ok(TargetConfig {
            out: out.to_string(),
        })
    })?;
    let mut output = OutputConfig::default();
    if let Some(v) = r.optional(root, "output") {
        let entries = r.map(v, "output", &["non_tty_format"])?;
        if let Some(v) = r.optional(entries, "non_tty_format") {
            let token = r.scalar(v, "output.non_tty_format")?;
            output.non_tty_format = Some(NonTtyFormat::parse(file, token)?);
        }
    }
    Ok(Config {
        protos,
        targets,
        output,
    })
}

fn decode_lock(file: &str, doc: &Value) -> Result<Lock, ProjectError> {
    let r = Reader { file };
    let root = r.map(doc, "", &["version", "spec", "targets"])?;
    let spec = r.map(r.required(root, "", "spec")?, "spec", &["commit"])?;
    let commit = r.scalar(r.required(spec, "spec", "commit")?, "spec.commit")?;
    let commit = SpecCommit::parse(file, commit)?;
    let targets = r.targets(root, |r, v, key| {
        let entries = r.map(v, key, &["client_api"])?;
        let api = r.scalar(
            r.required(entries, key, "client_api")?,
            &join(key, "client_api"),
        )?;
        Ok(TargetLock {
            client_api: api.to_string(),
        })
    })?;
    Ok(Lock {
        spec: SpecLock { commit },
        targets,
    })
}

/// `config.yaml`'s text, read from `file` (named in failures).
pub fn parse_config(file: &str, text: &str) -> Result<Config, Box<dyn Failure>> {
    let doc = config_loader()
        .load(Some(file), text)
        .map_err(ProjectFileError::loading)?;
    Ok(decode_config(file, &doc).map_err(ProjectInvalid)?)
}

/// `lock.yaml`'s text, read from `file` (named in failures).
pub fn parse_lock(file: &str, text: &str) -> Result<Lock, Box<dyn Failure>> {
    let doc = lock_loader()
        .load(Some(file), text)
        .map_err(ProjectFileError::loading)?;
    Ok(decode_lock(file, &doc).map_err(ProjectInvalid)?)
}

fn scalar(s: impl Into<String>) -> Value {
    Value::Scalar(s.into())
}

fn map(entries: Vec<(&str, Value)>) -> Value {
    Value::Map(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

/// Every collection in block style; an empty one as `{}`.
fn write(doc: &Value) -> String {
    annals_yaml::write(doc, usize::MAX)
}

/// `config.yaml`'s text for `config`, at the current version.
pub fn write_config(config: &Config) -> String {
    let targets = config
        .targets
        .iter()
        .map(|(t, c)| {
            (
                t.token().to_string(),
                map(vec![("out", scalar(c.out.as_str()))]),
            )
        })
        .collect();
    let mut root = vec![
        ("version", scalar(CONFIG_VERSION.to_string())),
        ("protos", scalar(config.protos.as_str())),
        ("targets", Value::Map(targets)),
    ];
    if let Some(format) = config.output.non_tty_format {
        root.push((
            "output",
            map(vec![("non_tty_format", scalar(format.token()))]),
        ));
    }
    write(&map(root))
}

/// `lock.yaml`'s text for `lock`, at the current version.
pub fn write_lock(lock: &Lock) -> String {
    let targets = lock
        .targets
        .iter()
        .map(|(t, l)| {
            (
                t.token().to_string(),
                map(vec![("client_api", scalar(l.client_api.as_str()))]),
            )
        })
        .collect();
    write(&map(vec![
        ("version", scalar(LOCK_VERSION.to_string())),
        (
            "spec",
            map(vec![("commit", scalar(lock.spec.commit.as_str()))]),
        ),
        ("targets", Value::Map(targets)),
    ]))
}

/// `.angzarr/` under a project root on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirProject {
    root: PathBuf,
}

impl DirProject {
    /// The project whose root is `root`; its directory need not exist.
    pub fn at(root: impl Into<PathBuf>) -> DirProject {
        DirProject { root: root.into() }
    }

    /// The nearest project at or above `start`: the first directory, walking
    /// up, that holds `.angzarr/config.yaml`.
    pub fn discover(start: &Path) -> Option<DirProject> {
        start
            .ancestors()
            .find(|dir| dir.join(DIR).join(CONFIG_FILE).is_file())
            .map(DirProject::at)
    }

    /// The project root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, file: &str) -> PathBuf {
        self.root.join(DIR).join(file)
    }

    fn read(&self, file: &str) -> Result<(String, String), ProjectFileError> {
        let path = self.path(file);
        let name = path.display().to_string();
        match std::fs::read_to_string(&path) {
            Ok(text) => Ok((name, text)),
            Err(e) => Err(ProjectFileError::Read {
                file: name,
                kind: io_kind(e.kind()),
            }),
        }
    }

    fn write(&self, file: &str, text: &str) -> Result<(), ProjectFileError> {
        let path = self.path(file);
        std::fs::write(&path, text).map_err(|e| ProjectFileError::Write {
            file: path.display().to_string(),
            kind: io_kind(e.kind()),
        })
    }
}

impl Project for DirProject {
    fn dir(&self) -> String {
        self.root.join(DIR).display().to_string()
    }

    fn exists(&self) -> Result<bool, Box<dyn Failure>> {
        let dir = self.root.join(DIR);
        Ok(dir.try_exists().map_err(|e| ProjectFileError::Read {
            file: dir.display().to_string(),
            kind: io_kind(e.kind()),
        })?)
    }

    fn read_config(&self) -> Result<Config, Box<dyn Failure>> {
        let (name, text) = self.read(CONFIG_FILE)?;
        parse_config(&name, &text)
    }

    fn read_lock(&self) -> Result<Lock, Box<dyn Failure>> {
        let (name, text) = self.read(LOCK_FILE)?;
        parse_lock(&name, &text)
    }

    /// Makes the directory itself (not its parents), so a directory made
    /// by anyone else first is refused, not overwritten.
    fn create(&self, config: &Config, lock: &Lock) -> Result<(), Box<dyn Failure>> {
        let dir = self.root.join(DIR);
        match std::fs::create_dir(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(AppError::ProjectExists { dir: self.dir() }.into());
            }
            Err(e) => {
                return Err(ProjectFileError::Write {
                    file: dir.display().to_string(),
                    kind: io_kind(e.kind()),
                }
                .into());
            }
        }
        self.write(CONFIG_FILE, &write_config(config))?;
        self.write(LOCK_FILE, &write_lock(lock))?;
        self.write(GITIGNORE_FILE, GITIGNORE)?;
        Ok(())
    }
}

/// `.angzarr/`'s files held in memory, by file name, through the same YAML
/// as [`DirProject`]. Failures name each file `.angzarr/<name>`.
#[derive(Debug, Default)]
pub struct MemoryProject {
    files: RefCell<Option<BTreeMap<String, String>>>,
}

impl MemoryProject {
    /// No project directory.
    pub fn new() -> MemoryProject {
        MemoryProject::default()
    }

    /// A project directory holding `files`, each a name and its text.
    pub fn with_files(files: &[(&str, &str)]) -> MemoryProject {
        let files = files
            .iter()
            .map(|(name, text)| (name.to_string(), text.to_string()))
            .collect();
        MemoryProject {
            files: RefCell::new(Some(files)),
        }
    }

    /// The text of file `name`, if the directory holds it.
    pub fn file(&self, name: &str) -> Option<String> {
        self.files.borrow().as_ref()?.get(name).cloned()
    }

    fn read(&self, name: &str) -> Result<(String, String), ProjectFileError> {
        let path = format!("{DIR}/{name}");
        match self.file(name) {
            Some(text) => Ok((path, text)),
            None => Err(ProjectFileError::Read {
                file: path,
                kind: "not-found",
            }),
        }
    }
}

impl Project for MemoryProject {
    fn dir(&self) -> String {
        DIR.to_string()
    }

    fn exists(&self) -> Result<bool, Box<dyn Failure>> {
        Ok(self.files.borrow().is_some())
    }

    fn read_config(&self) -> Result<Config, Box<dyn Failure>> {
        let (name, text) = self.read(CONFIG_FILE)?;
        parse_config(&name, &text)
    }

    fn read_lock(&self) -> Result<Lock, Box<dyn Failure>> {
        let (name, text) = self.read(LOCK_FILE)?;
        parse_lock(&name, &text)
    }

    fn create(&self, config: &Config, lock: &Lock) -> Result<(), Box<dyn Failure>> {
        let mut files = self.files.borrow_mut();
        if files.is_some() {
            return Err(AppError::ProjectExists { dir: self.dir() }.into());
        }
        *files = Some(BTreeMap::from([
            (CONFIG_FILE.to_string(), write_config(config)),
            (LOCK_FILE.to_string(), write_lock(lock)),
            (GITIGNORE_FILE.to_string(), GITIGNORE.to_string()),
        ]));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use angzarr_cli_app::{InitRequest, init};
    use annals_yaml::versioned::Fixture;

    const SHA: &str = "b1f00d2710eb7bee60308f6df520893d782012a6";
    const CONFIG: &str = ".angzarr/config.yaml";
    const LOCK: &str = ".angzarr/lock.yaml";

    fn full_config() -> Config {
        let mut config = Config::new([Target::Rust, Target::Python]);
        config.protos = "api/proto".into();
        config.output.non_tty_format = Some(NonTtyFormat::Yaml);
        config
    }

    fn full_lock() -> Lock {
        let mut lock = Lock::new(SpecCommit::parse("t", SHA).unwrap());
        lock.targets.insert(
            Target::Rust,
            TargetLock {
                client_api: "1".into(),
            },
        );
        lock
    }

    fn config_err(text: &str) -> Box<dyn Failure> {
        parse_config(CONFIG, text).unwrap_err()
    }

    fn lock_err(text: &str) -> Box<dyn Failure> {
        parse_lock(LOCK, text).unwrap_err()
    }

    fn arg(d: &dyn Diagnostic, name: &str) -> String {
        match d.args().into_iter().find(|(n, _)| *n == name) {
            Some((_, Arg::Str(s))) => s,
            Some((_, Arg::Nested(inner))) => inner.code().to_string(),
            other => panic!("{name}: {other:?}"),
        }
    }

    #[test]
    fn a_config_round_trips_through_its_text() {
        for config in [full_config(), Config::new([])] {
            let text = write_config(&config);
            assert_eq!(parse_config(CONFIG, &text).unwrap(), config, "{text}");
        }
    }

    #[test]
    fn a_lock_round_trips_through_its_text() {
        for lock in [full_lock(), Lock::new(SpecCommit::parse("t", SHA).unwrap())] {
            let text = write_lock(&lock);
            assert_eq!(parse_lock(LOCK, &text).unwrap(), lock, "{text}");
        }
    }

    #[test]
    fn the_written_text_is_block_yaml_at_version_1() {
        assert_eq!(
            write_config(&full_config()),
            "version: 1\nprotos: api/proto\ntargets:\n  rust:\n    out: gen/rust\n  python:\n    out: gen/python\noutput:\n  non_tty_format: yaml\n"
        );
        assert_eq!(
            write_config(&Config::new([])),
            "version: 1\nprotos: proto\ntargets: {}\n"
        );
        assert_eq!(
            write_lock(&full_lock()),
            format!("version: 1\nspec:\n  commit: {SHA}\ntargets:\n  rust:\n    client_api: 1\n")
        );
    }

    #[test]
    fn both_documents_are_at_version_1_with_whole_chains() {
        assert_eq!(
            (config_loader().current(), config_loader().oldest()),
            (1, 1)
        );
        assert_eq!((lock_loader().current(), lock_loader().oldest()), (1, 1));
    }

    const CONFIG_V1: &str = include_str!("../fixtures/project/config-v1.yaml");
    const LOCK_V1: &str = include_str!("../fixtures/project/lock-v1.yaml");

    /// A version-1 file must keep loading to the same current-version tree
    /// and the same settings. When a schema moves on, the fixture stays and
    /// gains its expected current-version output.
    #[test]
    fn the_version_1_fixtures_load_to_their_expected_settings() {
        config_loader().assert_fixtures(&[Fixture {
            name: "fixtures/project/config-v1.yaml",
            input: CONFIG_V1,
            expected: None,
        }]);
        lock_loader().assert_fixtures(&[Fixture {
            name: "fixtures/project/lock-v1.yaml",
            input: LOCK_V1,
            expected: None,
        }]);
        assert_eq!(parse_config("f", CONFIG_V1).unwrap(), full_config());
        assert_eq!(parse_lock("f", LOCK_V1).unwrap(), full_lock());
    }

    #[test]
    fn output_and_targets_are_optional() {
        let config = parse_config(CONFIG, "version: 1\nprotos: proto\n").unwrap();
        assert_eq!(config, Config::new([]));
        let config =
            parse_config(CONFIG, "version: 1\nprotos: proto\ntargets:\noutput:\n").unwrap();
        assert_eq!(config, Config::new([]));
    }

    #[test]
    fn an_unknown_target_names_the_file() {
        let err = config_err("version: 1\nprotos: p\ntargets:\n  go:\n    out: gen/go\n");
        assert_eq!(err.code(), "angzarr-cli-core-project-unknown-target");
        assert_eq!(arg(&*err, "source"), CONFIG);
        assert_eq!(arg(&*err, "target"), "go");
        assert_eq!(err.remedy().unwrap().code(), "angzarr-cli-core-fix-target");
        let err = lock_err(&format!(
            "version: 1\nspec:\n  commit: {SHA}\ntargets:\n  go:\n    client_api: 1\n"
        ));
        assert_eq!(err.code(), "angzarr-cli-core-project-unknown-target");
        assert_eq!(arg(&*err, "source"), LOCK);
    }

    #[test]
    fn an_invalid_non_tty_format_is_refused() {
        let err = config_err("version: 1\nprotos: p\noutput:\n  non_tty_format: xml\n");
        assert_eq!(err.code(), "angzarr-cli-core-project-bad-format");
        assert_eq!(arg(&*err, "value"), "xml");
        assert_eq!(err.remedy().unwrap().code(), "angzarr-cli-core-fix-format");
    }

    #[test]
    fn a_malformed_commit_is_refused() {
        let err = lock_err("version: 1\nspec:\n  commit: b1f00d2\n");
        assert_eq!(err.code(), "angzarr-cli-core-project-bad-commit");
        assert_eq!(arg(&*err, "source"), LOCK);
        assert_eq!(arg(&*err, "value"), "b1f00d2");
    }

    #[test]
    fn an_unknown_key_is_refused_with_its_path_and_file() {
        for (text, key) in [
            ("version: 1\nprotos: p\ncolour: red\n", "colour"),
            (
                "version: 1\nprotos: p\noutput:\n  colour: red\n",
                "output.colour",
            ),
            (
                "version: 1\nprotos: p\ntargets:\n  rust:\n    out: o\n    dir: d\n",
                "targets.rust.dir",
            ),
        ] {
            let err = config_err(text);
            assert_eq!(err.code(), "angzarr-cli-core-project-unknown-key", "{text}");
            assert_eq!(arg(&*err, "key"), key);
            assert_eq!(arg(&*err, "file"), CONFIG);
            assert_eq!(
                err.remedy().unwrap().code(),
                "angzarr-cli-core-fix-unknown-key"
            );
        }
        let err = lock_err(&format!(
            "version: 1\nspec:\n  commit: {SHA}\n  branch: main\n"
        ));
        assert_eq!(arg(&*err, "key"), "spec.branch");
    }

    #[test]
    fn a_missing_required_key_is_named() {
        for (text, key) in [
            ("version: 1\n", "protos"),
            ("version: 1\nprotos:\n", "protos"),
            (
                "version: 1\nprotos: p\ntargets:\n  rust:\n",
                "targets.rust.out",
            ),
        ] {
            let err = config_err(text);
            assert_eq!(err.code(), "angzarr-cli-core-project-missing-key", "{text}");
            assert_eq!(arg(&*err, "key"), key);
        }
        for (text, key) in [
            ("version: 1\n", "spec"),
            ("version: 1\nspec:\n  commit:\n", "spec.commit"),
        ] {
            assert_eq!(arg(&*lock_err(text), "key"), key, "{text}");
        }
    }

    #[test]
    fn a_value_of_the_wrong_shape_is_refused() {
        let err = config_err("version: 1\nprotos: p\ntargets: rust\n");
        assert_eq!(err.code(), "angzarr-cli-core-project-wrong-shape");
        assert_eq!(arg(&*err, "key"), "targets");
        assert_eq!(arg(&*err, "shape"), "mapping");
        let err = config_err("version: 1\nprotos: [a, b]\n");
        assert_eq!(arg(&*err, "shape"), "scalar");
    }

    #[test]
    fn a_newer_version_is_refused_with_annals_too_new_as_the_cause() {
        for err in [
            config_err("version: 2\nprotos: p\n"),
            lock_err("version: 2\n"),
        ] {
            assert_eq!(err.code(), "angzarr-cli-adapters-project-too-new");
            assert_eq!(arg(&*err, "cause"), "annals-yaml-too-new");
            assert_eq!(
                err.remedy().unwrap().code(),
                "angzarr-cli-adapters-fix-upgrade"
            );
        }
        let err = config_err("version: 2\n");
        assert_eq!(
            err.to_string(),
            ".angzarr/config.yaml was written by a newer angzarr: version 2 was written by a newer build; this build reads up to version 1"
        );
    }

    #[test]
    fn text_that_is_not_a_document_is_a_load_failure_with_the_cause() {
        let err = config_err("version: 1\n\tprotos: p\n");
        assert_eq!(err.code(), "angzarr-cli-adapters-project-load");
        assert_eq!(arg(&*err, "file"), CONFIG);
        assert_eq!(arg(&*err, "cause"), "annals-yaml-tab-indent");
        assert!(err.remedy().is_none());
        assert_eq!(
            arg(&*config_err("version: one\n"), "cause"),
            "annals-yaml-version-not-an-integer"
        );
    }

    #[test]
    fn a_memory_project_is_created_once_and_reads_back() {
        let project = MemoryProject::new();
        assert!(!project.exists().unwrap());
        let err = project.read_config().unwrap_err();
        assert_eq!(err.code(), "angzarr-cli-adapters-project-read");
        let request = InitRequest {
            targets: vec!["rust".into()],
            spec_commit: SHA.into(),
            dry_run: false,
        };
        let report = init(&project, &request).unwrap();
        assert_eq!(report.files, angzarr_cli_core::project::files());
        assert_eq!(project.read_config().unwrap(), report.config);
        assert_eq!(project.read_lock().unwrap(), report.lock);
        assert_eq!(project.file(".gitignore").as_deref(), Some("cache/\n"));
        let err = init(&project, &request).unwrap_err();
        assert_eq!(err.code(), "angzarr-cli-app-project-exists");
    }

    #[test]
    fn a_memory_project_reads_hand_written_files() {
        let project = MemoryProject::with_files(&[(CONFIG_FILE, CONFIG_V1), (LOCK_FILE, LOCK_V1)]);
        assert_eq!(project.read_config().unwrap(), full_config());
        assert_eq!(project.read_lock().unwrap(), full_lock());
    }
}
