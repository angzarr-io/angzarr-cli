//! `angzarr`: the command line for angzarr projects.

use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use angzarr_build::{
    AppSeed, BuildError, DomainCrate, Existing, Exposure, Notes, ProtoInput, PythonApp, RetrySeed,
    Scaffolded,
};
use angzarr_codegen::{ANGZARR_BUILD_GIT, ANGZARR_BUILD_REV};
use angzarr_fmt::Viewable;
use angzarr_presentation::{Catalog, CatalogError, Format};
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use cli_support::{
    Exit, ExitClass, ExitStatuses, Failure, detailed_exitcode_arg, exit_statuses, say_notices,
    usage_exit,
};
use serde::Serialize;

use angzarr_diagnostic::{Arg, Diagnostic, code};

mod output;
mod probe;

use output::{FormatArgs, PROG, finish, finish_shown};

#[derive(Parser)]
#[command(name = "angzarr", about = "The command line for angzarr projects.")]
struct Cli {
    #[command(flatten)]
    format: FormatArgs,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show which build of the CLI this is.
    Version,
    /// Write each component's markup scaffold, once: the user's file, never
    /// overwritten.
    Scaffold(Box<ScaffoldArgs>),
    /// Check a project's files against its protos.
    Lint(LintArgs),
    /// Take one component of the host config through the child protocol,
    /// a step at a time, and show how each step went.
    Probe(Box<probe::ProbeArgs>),
}

#[derive(Args)]
struct LintArgs {
    /// What to check: `app`, the host config.
    #[arg(value_enum)]
    what: LintWhat,
    /// The protos declaring the components, as import names under an
    /// include.
    #[arg(required = true)]
    protos: Vec<String>,
    /// Where the protos' imports are found; angzarr's own are always found.
    #[arg(short = 'I', long = "include", value_name = "DIR")]
    includes: Vec<String>,
    /// The host config.
    #[arg(long, default_value = "angzarr.yaml")]
    config: PathBuf,
}

#[derive(Clone, Copy, ValueEnum)]
enum LintWhat {
    /// The host config, against the protos' components.
    App,
}

/// A lint run that found nothing.
#[derive(Serialize)]
struct LintReport {
    config: String,
    lints: usize,
    /// The protos' declaration warnings, which do not fail the command.
    warnings: usize,
}

impl Viewable for LintReport {}

#[derive(Args)]
struct ScaffoldArgs {
    /// The language to scaffold.
    #[arg(value_enum)]
    lang: Lang,
    /// The protos declaring the components, as import names under an
    /// include.
    #[arg(required = true)]
    protos: Vec<String>,
    /// Where the protos' imports are found; angzarr's own are always found.
    #[arg(short = 'I', long = "include", value_name = "DIR")]
    includes: Vec<String>,
    /// Where the files go: a Rust crate's source directory, or a Python
    /// project's root.
    #[arg(long, default_value = "src")]
    src: PathBuf,
    /// Add the required methods an existing file lacks, changing nothing
    /// else.
    #[arg(long)]
    missing: bool,
    #[command(flatten)]
    python: PythonArgs,
    #[command(flatten)]
    app: AppArgs,
}

/// What `scaffold app` alone takes: the seed of a new host config, unused
/// once `angzarr.yaml` exists (it is the user's).
#[derive(Args)]
struct AppArgs {
    /// App: the application's name, for the generated prose.
    #[arg(long, default_value = "the application")]
    name: String,
    /// App: a component's language, `<name>=<language>`, the component by
    /// its name in the application (a domain also by its domain): `python`
    /// is served by a child process, the project of `scaffold python`;
    /// every component not named is Rust, in process. Repeatable.
    #[arg(long = "language", value_name = "NAME=LANGUAGE")]
    languages: Vec<String>,
    /// App: a domain exposed through the host's listener,
    /// `<domain>=loopback` or `<domain>=ip`. Repeatable.
    #[arg(long, value_name = "DOMAIN=EXPOSURE")]
    expose: Vec<String>,
    /// App: `retry.max_retries`; unset, the host's default.
    #[arg(long, value_name = "N")]
    max_retries: Option<u32>,
    /// App: `retry.max_race_runs`; 0 hands a lost race back as the
    /// store's conflict. Unset, the host's default.
    #[arg(long, value_name = "N")]
    max_race_runs: Option<u32>,
    /// App: an in-memory store, else the SQLite file `angzarr.db`.
    #[arg(long)]
    in_memory: bool,
    /// App: the manifest's angzarr-build is this directory's, else the
    /// angzarr-build repository at the revision this binary is built
    /// against.
    #[arg(long, value_name = "DIR")]
    angzarr_build_path: Option<PathBuf>,
    /// App: the library crate whose domain layer the application uses
    /// (its `domain` module, from angzarr-build's `wire` and
    /// `markup_domain`), as Rust names it, e.g. `examples_proto`; written
    /// into the manifest, and no domain layer is generated.
    #[arg(long, value_name = "CRATE")]
    domain_crate: Option<String>,
    /// App: the domain crate's wire types, as the application's
    /// `crate::wire`, e.g. `examples_proto::v1`.
    #[arg(long, value_name = "PATH", requires = "domain_crate")]
    domain_wire: Option<String>,
}

/// What `scaffold python` alone takes.
#[derive(Args)]
struct PythonArgs {
    /// Python: the domain the project serves, e.g. `player`, or a
    /// component's name, e.g. `policy-table-player`.
    #[arg(long)]
    domain: Option<String>,
    /// Python: the git repository `buf generate` reads the protos from,
    /// under its `proto`; unset, it reads them from the first `--include`.
    #[arg(long, value_name = "URL", requires = "proto_ref")]
    proto_git: Option<String>,
    /// Python: the revision of `--proto-git`.
    #[arg(long, value_name = "REV")]
    proto_ref: Option<String>,
    /// Python: the build version the domain reports as its rules_version.
    #[arg(long, default_value = "0.1.0")]
    rules_version: String,
    /// Python: skip `uv lock`, which otherwise ends every run.
    #[arg(long)]
    no_lock: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Lang {
    Rust,
    Python,
    /// The application's startup: its host config, build.rs, main.rs and
    /// driver, under `--src` (the crate root).
    App,
}

/// What a scaffold run did, one entry per component file.
#[derive(Serialize)]
struct ScaffoldReport {
    files: Vec<ScaffoldEntry>,
}

#[derive(Serialize)]
struct ScaffoldEntry {
    path: String,
    /// `created`, `completed`, `regenerated`, `kept`, `obsolete` (an earlier
    /// scaffold's file this one no longer writes, the user's to delete) or
    /// `deleted` (an orphaned generated file).
    action: &'static str,
    /// A user's file's required methods it lacks.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    lacks: Vec<String>,
    /// A user's file's methods for a message the protos no longer declare.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    obsolete: Vec<String>,
    /// The optional refusal callbacks a user's implementation may define and
    /// does not (`--on-refused` writes one).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    optional: Vec<String>,
    /// A user's manifest's pins that are not this CLI's: reported, the
    /// manifest never edited.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    drift: Vec<Drift>,
}

/// A dependency a manifest pins at another revision than this CLI's.
#[derive(Serialize)]
struct Drift {
    dependency: String,
    pinned: String,
    cli: String,
}

impl Viewable for ScaffoldReport {}

fn scaffold(args: ScaffoldArgs) -> Result<ScaffoldReport, Failure> {
    let protos: Vec<&str> = args.protos.iter().map(String::as_str).collect();
    let includes: Vec<&str> = args.includes.iter().map(String::as_str).collect();
    let existing = if args.missing {
        Existing::Complete
    } else {
        Existing::Keep
    };
    let done = match args.lang {
        Lang::Rust => angzarr_build::scaffold_into(&args.src, &protos, &includes, existing)
            .map_err(CliError::Build)?,
        Lang::Python => {
            let python = &args.python;
            let domain = python.domain.clone().ok_or(CliError::DomainRequired)?;
            let protos_input = match (&python.proto_git, &python.proto_ref) {
                (Some(url), Some(rev)) => ProtoInput::Git {
                    url: url.clone(),
                    rev: rev.clone(),
                },
                _ => ProtoInput::Directory(from_root(
                    &args.src,
                    includes.first().copied().unwrap_or("."),
                )),
            };
            let app = PythonApp {
                domain,
                protos: protos_input,
                rules_version: python.rules_version.clone(),
            };
            let done =
                angzarr_build::scaffold_python_into(&args.src, &protos, &includes, &app, existing)
                    .map_err(CliError::Build)?;
            angzarr_build::proto_gen_python(&args.src, &app).map_err(CliError::Build)?;
            if !python.no_lock {
                angzarr_build::lock_python(&args.src).map_err(CliError::Build)?;
            }
            done
        }
        Lang::App => {
            let expose = args
                .app
                .expose
                .iter()
                .map(String::as_str)
                .map(exposure)
                .collect::<Result<Vec<_>, _>>()?;
            let languages = args
                .app
                .languages
                .iter()
                .map(String::as_str)
                .map(language)
                .collect::<Result<Vec<_>, _>>()?;
            let seed = AppSeed {
                name: args.app.name.clone(),
                includes: args.includes.clone(),
                languages,
                expose,
                retry: RetrySeed {
                    max_retries: args.app.max_retries,
                    max_race_runs: args.app.max_race_runs,
                },
                in_memory: args.app.in_memory,
                angzarr_build: Some(angzarr_build_dependency(
                    args.app.angzarr_build_path.as_deref(),
                )),
                domain: args.app.domain_crate.clone().map(|path| DomainCrate {
                    path,
                    wire: args.app.domain_wire.clone(),
                }),
            };
            angzarr_build::app_scaffold_into(&args.src, &protos, &includes, &seed)
                .map_err(CliError::Build)?
        }
    };
    Ok(report(done))
}

/// The directory `dir`, which a relative `-I` names from where the CLI
/// runs, as buf reads it in the project `root`: an absolute one as it is, a
/// relative one from `root`.
fn from_root(root: &Path, dir: &str) -> String {
    if Path::new(dir).is_absolute() {
        return dir.to_string();
    }
    let (Ok(from), Ok(to)) = (std::path::absolute(root), std::path::absolute(dir)) else {
        return dir.to_string();
    };
    let (from, to) = (lexical(&from), lexical(&to));
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let parts: Vec<&str> = std::iter::repeat_n("..", from.len() - common)
        .chain(to[common..].iter().map(String::as_str))
        .collect();
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

/// An absolute `path`'s components, each `..` taking away the one before.
fn lexical(path: &Path) -> Vec<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        if component == Component::ParentDir {
            parts.pop();
        } else {
            parts.push(component.as_os_str().to_string_lossy().into_owned());
        }
    }
    parts
}

/// One entry per file, by what was done with it, a user's file's notes
/// beside it.
fn report(done: Vec<Scaffolded>) -> ScaffoldReport {
    let files = done
        .into_iter()
        .map(|d| {
            let (path, action, notes) = match d {
                Scaffolded::Created(path) => (path, "created", Notes::default()),
                Scaffolded::Completed { path, notes } => (path, "completed", notes),
                Scaffolded::Regenerated(path) => (path, "regenerated", Notes::default()),
                Scaffolded::Kept { path, notes } => (path, "kept", notes),
                Scaffolded::Obsolete(path) => (path, "obsolete", Notes::default()),
                Scaffolded::Deleted(path) => (path, "deleted", Notes::default()),
            };
            ScaffoldEntry {
                path: path.display().to_string(),
                action,
                lacks: notes.lacks,
                obsolete: notes.obsolete,
                optional: notes.optional,
                drift: notes
                    .drift
                    .into_iter()
                    .map(|d| Drift {
                        dependency: d.dependency,
                        pinned: d.pinned,
                        cli: d.cli,
                    })
                    .collect(),
            }
        })
        .collect();
    ScaffoldReport { files }
}

/// `angzarr lint app`: each lint of the config and each of the protos'
/// declaration warnings on stderr as notices, and a failure when there is
/// any lint.
fn lint(args: LintArgs, format: Format, catalog: &Catalog) -> Result<LintReport, Failure> {
    let LintWhat::App = args.what;
    let protos: Vec<&str> = args.protos.iter().map(String::as_str).collect();
    let includes: Vec<&str> = args.includes.iter().map(String::as_str).collect();
    let linted =
        angzarr_build::lint_app(&args.config, &protos, &includes).map_err(CliError::Build)?;
    // A declaration warning leaves the exit status as it is; a lint of the
    // config fails it.
    let mut err = std::io::stderr().lock();
    say_notices(&linted.lints, PROG, format, catalog, &mut err);
    say_notices(&linted.warnings, PROG, format, catalog, &mut err);
    if !linted.lints.is_empty() {
        return Err(CliError::Lints {
            config: args.config.display().to_string(),
            count: linted.lints.len(),
        }
        .into());
    }
    Ok(LintReport {
        config: args.config.display().to_string(),
        lints: 0,
        warnings: linted.warnings.len(),
    })
}

/// The generated manifest's angzarr-build: the directory given, else this
/// generator's pin of it ([`ANGZARR_BUILD_GIT`] at [`ANGZARR_BUILD_REV`],
/// the pair the generated applications' manifests name).
fn angzarr_build_dependency(path: Option<&Path>) -> String {
    match path {
        Some(path) => format!("{{ path = {:?} }}", path.display().to_string()),
        None => format!("{{ git = \"{ANGZARR_BUILD_GIT}\", rev = \"{ANGZARR_BUILD_REV}\" }}"),
    }
}

/// `<domain>=loopback` or `<domain>=ip` as an exposure.
fn exposure(value: &str) -> Result<(String, Exposure), CliError> {
    let malformed = || CliError::ExposeMalformed {
        value: value.to_string(),
    };
    let (domain, how) = value.split_once('=').ok_or_else(malformed)?;
    let how = match how {
        "loopback" => Exposure::Loopback,
        "ip" => Exposure::Ip,
        _ => return Err(malformed()),
    };
    if domain.is_empty() {
        return Err(malformed());
    }
    Ok((domain.to_string(), how))
}

/// `<name>=<language>` as a component's language.
fn language(value: &str) -> Result<(String, String), CliError> {
    match value.split_once('=') {
        Some((name, language)) if !name.is_empty() && !language.is_empty() => {
            Ok((name.to_string(), language.to_string()))
        }
        _ => Err(CliError::LanguageMalformed {
            value: value.to_string(),
        }),
    }
}

/// Why a command failed.
#[derive(Debug)]
enum CliError {
    /// `scaffold python` without `--domain`.
    DomainRequired,
    /// An `--expose` value not of the form `<domain>=loopback|ip`.
    ExposeMalformed { value: String },
    /// A `--language` value not of the form `<name>=<language>`.
    LanguageMalformed { value: String },
    /// `lint` found `count` problems in `config`.
    Lints { config: String, count: usize },
    /// Generation failed; reported as the build error itself.
    Build(BuildError),
    /// The host refused: its config unreadable or invalid; reported as
    /// the host's error itself.
    Host(angzarr_embedded_host::HostError),
    /// `probe` named a component `config` does not list.
    ProbeNotListed { component: String, config: String },
    /// `probe` gave a component of `kind` (its kind in the config), which
    /// is handed an input, none.
    ProbeNoInput { component: String, kind: String },
    /// `probe`'s `--payload` file could not be read. `kind` is the I/O
    /// error's `std::io::ErrorKind`, by name.
    ProbePayloadUnreadable { path: String, kind: String },
    /// A step of `probe` failed: the first, `step`.
    ProbeFailed { component: String, step: String },
}

/// Its code and args (decision 14.37), a build's error as that error's;
/// [`finish`] shows it in English, through the catalog.
impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        angzarr_diagnostic::display(self, f)
    }
}

impl std::error::Error for CliError {}

impl Diagnostic for CliError {
    fn code(&self) -> &str {
        match self {
            CliError::DomainRequired => code!("domain-required"),
            CliError::ExposeMalformed { .. } => code!("expose-malformed"),
            CliError::LanguageMalformed { .. } => code!("language-malformed"),
            CliError::Lints { .. } => code!("lints"),
            CliError::Build(error) => error.code(),
            CliError::Host(error) => error.code(),
            CliError::ProbeNotListed { .. } => code!("probe-not-listed"),
            CliError::ProbeNoInput { .. } => code!("probe-no-input"),
            CliError::ProbePayloadUnreadable { .. } => code!("probe-payload-unreadable"),
            CliError::ProbeFailed { .. } => code!("probe-failed"),
        }
    }

    fn args(&self) -> Vec<(&str, Arg)> {
        match self {
            CliError::DomainRequired => Vec::new(),
            CliError::ExposeMalformed { value } | CliError::LanguageMalformed { value } => {
                vec![("value", value.as_str().into())]
            }
            CliError::Lints { config, count } => vec![
                ("config", config.as_str().into()),
                ("count", i64::try_from(*count).unwrap_or(i64::MAX).into()),
            ],
            CliError::Build(error) => error.args(),
            CliError::Host(error) => error.args(),
            CliError::ProbeNotListed { component, config } => vec![
                ("component", component.as_str().into()),
                ("config", config.as_str().into()),
            ],
            CliError::ProbeNoInput { component, kind } => vec![
                ("component", component.as_str().into()),
                ("kind", kind.as_str().into()),
            ],
            CliError::ProbePayloadUnreadable { path, kind } => vec![
                ("path", path.as_str().into()),
                ("kind", kind.as_str().into()),
            ],
            CliError::ProbeFailed { component, step } => vec![
                ("component", component.as_str().into()),
                ("step", step.as_str().into()),
            ],
        }
    }
}

/// The exit each error ends the command with: a request the line or the
/// state forbids is declined (20), the lints of a config are findings that
/// cannot produce a working result (21), and anything that broke on the way
/// failed (10). Its remedy, where it has one, is the catalog's message
/// `<code>-fix` ([`cli_support::report_failure`]).
impl ExitClass for CliError {
    fn exit(&self) -> Exit {
        match self {
            CliError::DomainRequired
            | CliError::ExposeMalformed { .. }
            | CliError::LanguageMalformed { .. }
            | CliError::ProbeNotListed { .. }
            | CliError::ProbeNoInput { .. } => Exit::REFUSED,
            CliError::Lints { .. } => Exit::FATAL_FINDINGS,
            CliError::Build(_)
            | CliError::Host(_)
            | CliError::ProbePayloadUnreadable { .. }
            | CliError::ProbeFailed { .. } => Exit::FAILED,
        }
    }
}

impl From<CliError> for Failure {
    fn from(error: CliError) -> Self {
        Failure::of(error)
    }
}

/// This crate's en-US messages: its package name, and its catalog, each id
/// under that name (written by the build, `angzarr_diagnostic::build`).
const EN_US: (&str, &str) = (
    env!("CARGO_PKG_NAME"),
    include_str!(concat!(env!("OUT_DIR"), "/en-US.ftl")),
);

/// Which build of the CLI this is.
#[derive(Serialize)]
struct Version {
    name: &'static str,
    version: &'static str,
    /// `git describe` of the checkout it was built from, when there was one.
    commit: Option<&'static str>,
}

impl Viewable for Version {}

fn version() -> Version {
    Version {
        name: "angzarr",
        version: env!("CARGO_PKG_VERSION"),
        commit: option_env!("ANGZARR_COMMIT"),
    }
}

/// The remedies angzarr-build names, aliased to the ids a remedy is looked
/// up by (`catalogs/build-fixes.ftl`).
const BUILD_FIXES: (&str, &str) = (
    "angzarr-cli-build-fixes",
    include_str!("../catalogs/build-fixes.ftl"),
);

/// Every crate's en-US messages.
fn catalog() -> Result<Catalog, CatalogError> {
    Catalog::en_us(&[
        angzarr_fmt::EN_US,
        angzarr_codegen::EN_US,
        angzarr_build::EN_US,
        // The probe's refusals to start: the host's own, a config's
        // reader's, and the spec's keys.
        angzarr_embedded_host::EN_US,
        angzarr_embedded_host::angzarr_embedded_host_config::angzarr_yaml::EN_US,
        angzarr_proto::REASONS_EN_US,
        angzarr_proto_transport_embedded::REASONS_EN_US,
        BUILD_FIXES,
        EN_US,
    ])
}

fn main() -> ExitCode {
    let matches = match Cli::command()
        .arg(detailed_exitcode_arg())
        .try_get_matches()
    {
        Ok(matches) => matches,
        Err(e) => {
            let _ = e.print();
            return ExitStatuses::new(false).process(usage_exit(&e));
        }
    };
    let statuses = exit_statuses(&matches);
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(e) => {
            let _ = e.print();
            return statuses.process(usage_exit(&e));
        }
    };
    let catalog = match catalog() {
        Ok(catalog) => catalog,
        Err(e) => {
            eprintln!("{PROG}: {e}");
            return statuses.process(Exit::FAILED);
        }
    };
    let format = cli.format.resolve();
    match cli.command {
        Command::Version => finish(format, statuses, &catalog, Ok(version())),
        Command::Scaffold(args) => finish(format, statuses, &catalog, scaffold(*args)),
        Command::Lint(args) => {
            let result = lint(args, format, &catalog);
            finish(format, statuses, &catalog, result)
        }
        Command::Probe(args) => finish_shown(
            format,
            statuses,
            &catalog,
            probe::run(*args),
            probe::verdict,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use angzarr_diagnostic::Report;

    /// One of every diagnostic this crate raises, for the catalog check.
    fn diagnostic_samples() -> Vec<Box<dyn Diagnostic>> {
        let domain = CliError::DomainRequired;
        let language = CliError::LanguageMalformed {
            value: "x".to_string(),
        };
        let no_input = CliError::ProbeNoInput {
            component: "player".to_string(),
            kind: "domain".to_string(),
        };
        vec![
            Box::new(Report::new(code!("domain-required-fix"))),
            Box::new(domain),
            Box::new(CliError::ExposeMalformed {
                value: "x".to_string(),
            }),
            Box::new(Report::new(code!("language-malformed-fix"))),
            Box::new(language),
            Box::new(CliError::Lints {
                config: "x".to_string(),
                count: 2,
            }),
            Box::new(CliError::ProbeNotListed {
                component: "dealer".to_string(),
                config: "angzarr.yaml".to_string(),
            }),
            Box::new(Report::new(code!("probe-no-input-fix"))),
            Box::new(no_input),
            Box::new(CliError::ProbePayloadUnreadable {
                path: "move.bin".to_string(),
                kind: "NotFound".to_string(),
            }),
            Box::new(CliError::ProbeFailed {
                component: "player".to_string(),
                step: "handshake".to_string(),
            }),
        ]
    }

    /// Every error displays as its code and args, never English (decision
    /// 14.37), and core's catalog check agrees with the catalog; a build's
    /// error displays as that error.
    #[test]
    fn every_error_displays_as_its_code_and_args() {
        let (krate, ftl) = EN_US;
        let problems = angzarr_diagnostic::catalog::check(krate, ftl, &diagnostic_samples());
        assert_eq!(problems, Vec::new());
        let build = BuildError::Uv {
            detail: "x".to_string(),
        };
        assert_eq!(
            CliError::Build(build.clone()).to_string(),
            build.to_string()
        );
    }

    #[test]
    fn the_catalogs_of_every_crate_combine() {
        catalog().unwrap();
    }

    /// Every sample, and the remedy of each error that has one, has a
    /// message in the program's catalog.
    #[test]
    fn the_programs_catalog_renders_every_error_and_its_remedy() {
        let catalog = catalog().unwrap();
        for sample in diagnostic_samples() {
            let message = catalog.render(&*sample);
            assert!(message.is_ok(), "{sample}: {message:?}");
        }
    }

    /// A remedy angzarr-build names under its own code is reported under
    /// the failure's `-fix`: the same text, said as the failure's fix.
    #[test]
    fn a_build_errors_remedy_is_its_fix() {
        let unmarked = BuildError::GeneratedUnmarked {
            path: "build.rs".to_string(),
        };
        let remedy = unmarked.remedy().expect("a remedy");
        let catalog = catalog().unwrap();
        let failure = Failure::of(CliError::Build(unmarked));
        let mut said = Vec::new();
        failure
            .say(PROG, Format::Text, &catalog, &mut said)
            .unwrap();
        let said = String::from_utf8(said).unwrap();
        assert!(
            said.ends_with(&format!("  fix: {}\n", catalog.message(&remedy))),
            "{said}"
        );
    }

    #[test]
    fn a_refusal_is_declined_lints_are_findings_and_a_breakage_failed() {
        assert_eq!(CliError::DomainRequired.exit(), Exit::REFUSED);
        let lints = CliError::Lints {
            config: "angzarr.yaml".to_string(),
            count: 1,
        };
        assert_eq!(lints.exit(), Exit::FATAL_FINDINGS);
        let build = CliError::Build(BuildError::Uv {
            detail: "x".to_string(),
        });
        assert_eq!(build.exit(), Exit::FAILED);
    }

    #[test]
    fn the_manifests_angzarr_build_is_the_path_else_the_pinned_revision() {
        assert_eq!(
            angzarr_build_dependency(Some(Path::new("/x/angzarr-build"))),
            "{ path = \"/x/angzarr-build\" }"
        );
        assert_eq!(
            angzarr_build_dependency(None),
            format!("{{ git = \"{ANGZARR_BUILD_GIT}\", rev = \"{ANGZARR_BUILD_REV}\" }}")
        );
    }
}
