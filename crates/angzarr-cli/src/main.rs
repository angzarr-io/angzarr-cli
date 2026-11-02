//! `angzarr`: the command line for angzarr projects.

use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use angzarr_build::{
    AppSeed, BuildError, DomainCrate, Existing, Exposure, Notes, ProtoInput, PythonApp, RetrySeed,
    Scaffolded, Translate,
};
use angzarr_fmt::{Catalog, Failure, Format, Viewable};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;

use angzarr_diagnostic::{Arg, Diagnostic, Report, code};

mod output;
mod probe;

use output::{FormatArgs, finish, finish_shown};

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
    /// The fact a policy's refusal callback for one command returns in its
    /// scaffolded stub: `<policy>:<command>=<fact>`, each fully qualified.
    /// Every command of a policy's target has a required callback
    /// (decision 14.76); the Python stub of one this names returns that
    /// fact, any other the source's fact choice.
    #[arg(long, value_name = "POLICY:COMMAND=FACT")]
    translate: Vec<String>,
    /// Write a Python domain's optional refusal handler for one refused
    /// command (`on_refused_<command>`) into the implementation, by the
    /// command's full name, even when the file exists. Repeatable; without
    /// it a run only reports them. A Rust implementation has none.
    #[arg(long, value_name = "COMMAND")]
    on_refused: Vec<String>,
    /// Add the required methods an existing file lacks, changing nothing
    /// else; a domain's optional refusal handlers are added only by
    /// `--on-refused`.
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
    /// angzarr-cli repository at the commit this binary was built from.
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

fn scaffold(args: ScaffoldArgs) -> Result<ScaffoldReport, Box<dyn Failure>> {
    let protos: Vec<&str> = args.protos.iter().map(String::as_str).collect();
    let includes: Vec<&str> = args.includes.iter().map(String::as_str).collect();
    let existing = if args.missing {
        Existing::Complete
    } else {
        Existing::Keep
    };
    let translate = args
        .translate
        .iter()
        .map(String::as_str)
        .map(translate)
        .collect::<Result<Vec<_>, _>>()?;
    let done = match args.lang {
        Lang::Rust => {
            angzarr_build::scaffold_into(&args.src, &protos, &includes, &translate, existing)
                .map_err(CliError::Build)?
        }
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
            let done = angzarr_build::scaffold_python_into(
                &args.src,
                &protos,
                &includes,
                &app,
                &translate,
                &args.on_refused,
                existing,
            )
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
                    option_env!("ANGZARR_COMMIT"),
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
/// declaration warnings on stderr, and a failure when there is any lint.
fn lint(args: LintArgs, catalog: &Catalog) -> Result<LintReport, Box<dyn Failure>> {
    let LintWhat::App = args.what;
    let protos: Vec<&str> = args.protos.iter().map(String::as_str).collect();
    let includes: Vec<&str> = args.includes.iter().map(String::as_str).collect();
    let linted =
        angzarr_build::lint_app(&args.config, &protos, &includes).map_err(CliError::Build)?;
    // angzarr-fmt has no warning of its own: each lint and each warning is
    // one line, its code and its message. A declaration warning leaves the
    // exit status as it is; a lint of the config fails it.
    for lint in &linted.lints {
        eprintln!(
            "angzarr: warning[{}]: {}",
            lint.code(),
            catalog.render(lint)
        );
    }
    for warning in &linted.warnings {
        eprintln!(
            "angzarr: warning[{}]: {}",
            warning.code(),
            catalog.render(warning)
        );
    }
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

/// The generated manifest's angzarr-build: the directory given, else the
/// angzarr-cli repository at `commit` when it is one (hex, not `-dirty`),
/// else its default branch.
fn angzarr_build_dependency(path: Option<&Path>, commit: Option<&str>) -> String {
    match (path, commit) {
        (Some(path), _) => format!("{{ path = {:?} }}", path.display().to_string()),
        (None, Some(commit))
            if !commit.is_empty() && commit.chars().all(|c| c.is_ascii_hexdigit()) =>
        {
            format!(
                "{{ git = \"{}\", rev = \"{commit}\" }}",
                angzarr_build::ANGZARR_BUILD_GIT
            )
        }
        _ => format!("{{ git = \"{}\" }}", angzarr_build::ANGZARR_BUILD_GIT),
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

/// `<policy>:<command>=<fact>` as a translate selection.
fn translate(value: &str) -> Result<Translate, CliError> {
    let malformed = || CliError::TranslateMalformed {
        value: value.to_string(),
    };
    let (policy, rest) = value.split_once(':').ok_or_else(malformed)?;
    let (command, fact) = rest.split_once('=').ok_or_else(malformed)?;
    if [policy, command, fact].iter().any(|part| part.is_empty()) {
        return Err(malformed());
    }
    Ok(Translate {
        policy: policy.to_string(),
        command: command.to_string(),
        fact: fact.to_string(),
    })
}

/// Why a command failed.
#[derive(Debug)]
enum CliError {
    /// A `--translate` value not of the form `<policy>:<command>=<fact>`.
    TranslateMalformed { value: String },
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
            CliError::TranslateMalformed { .. } => code!("translate-malformed"),
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
            CliError::TranslateMalformed { value } => vec![("value", value.as_str().into())],
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

impl Failure for CliError {
    fn remedy(&self) -> Option<Report> {
        match self {
            CliError::TranslateMalformed { .. } => Some(Report::new(code!("translate-form"))),
            CliError::DomainRequired => Some(Report::new(code!("domain-name"))),
            CliError::LanguageMalformed { .. } => Some(Report::new(code!("language-form"))),
            CliError::ProbeNoInput { .. } => Some(Report::new(code!("probe-input-form"))),
            CliError::Build(error) => error.remedy(),
            CliError::ExposeMalformed { .. }
            | CliError::Lints { .. }
            | CliError::Host(_)
            | CliError::ProbeNotListed { .. }
            | CliError::ProbePayloadUnreadable { .. }
            | CliError::ProbeFailed { .. } => None,
        }
    }
}

impl From<CliError> for Box<dyn Failure> {
    fn from(error: CliError) -> Self {
        Box::new(error)
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

/// Every crate's en-US messages.
fn catalog() -> Catalog {
    let mut catalog = Catalog::en_us();
    for ftl in [
        angzarr_fmt::EN_US.1,
        angzarr_codegen::EN_US.1,
        angzarr_build::EN_US.1,
        // The probe's refusals to start: the host's own, a config's
        // reader's, and the spec's keys.
        angzarr_embedded_host::EN_US.1,
        angzarr_embedded_host::angzarr_embedded_host_config::angzarr_yaml::EN_US.1,
        angzarr_proto::REASONS_EN_US.1,
        angzarr_proto_transport_embedded::REASONS_EN_US.1,
        EN_US.1,
    ] {
        catalog
            .add(ftl)
            .expect("every crate's catalog parses; its own tests check it");
    }
    catalog
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let format = cli.format.resolve(Format::Json);
    let catalog = catalog();
    match cli.command {
        Command::Version => finish(format, &catalog, Ok::<_, Box<dyn Failure>>(version())),
        Command::Scaffold(args) => finish(format, &catalog, scaffold(*args)),
        Command::Lint(args) => {
            let result = lint(args, &catalog);
            finish(format, &catalog, result)
        }
        Command::Probe(args) => finish_shown(format, &catalog, probe::run(*args), probe::verdict),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One of every diagnostic this crate raises, for the catalog check.
    fn diagnostic_samples() -> Vec<Box<dyn Diagnostic>> {
        let malformed = CliError::TranslateMalformed {
            value: "x".to_string(),
        };
        let remedy = malformed.remedy().expect("a remedy");
        let domain = CliError::DomainRequired;
        let domain_remedy = domain.remedy().expect("a remedy");
        let language = CliError::LanguageMalformed {
            value: "x".to_string(),
        };
        let language_remedy = language.remedy().expect("a remedy");
        let no_input = CliError::ProbeNoInput {
            component: "player".to_string(),
            kind: "domain".to_string(),
        };
        let no_input_remedy = no_input.remedy().expect("a remedy");
        vec![
            Box::new(malformed),
            Box::new(remedy),
            Box::new(domain),
            Box::new(domain_remedy),
            Box::new(CliError::ExposeMalformed {
                value: "x".to_string(),
            }),
            Box::new(language),
            Box::new(language_remedy),
            Box::new(CliError::Lints {
                config: "x".to_string(),
                count: 2,
            }),
            Box::new(CliError::ProbeNotListed {
                component: "dealer".to_string(),
                config: "angzarr.yaml".to_string(),
            }),
            Box::new(no_input),
            Box::new(no_input_remedy),
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

    #[test]
    fn every_error_has_a_catalog_message_with_its_args() {
        let problems = angzarr_fmt::check(EN_US.1, &diagnostic_samples());
        assert!(problems.is_empty(), "{problems:#?}");
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
        catalog();
    }

    #[test]
    fn translate_parses_policy_command_and_fact() {
        let parsed = translate("p.Policy:t.Cmd=s.Fact").unwrap();
        assert_eq!(
            (parsed.policy, parsed.command, parsed.fact),
            (
                "p.Policy".to_string(),
                "t.Cmd".to_string(),
                "s.Fact".to_string()
            )
        );
        for malformed in [
            "t.Cmd",
            "p.Policy:t.Cmd",
            ":t.Cmd=s.Fact",
            "p.Policy:=s.Fact",
        ] {
            assert!(translate(malformed).is_err(), "{malformed}");
        }
    }

    #[test]
    fn the_manifests_angzarr_build_is_the_path_else_the_commit_else_the_repository() {
        let git = angzarr_build::ANGZARR_BUILD_GIT;
        assert_eq!(
            angzarr_build_dependency(Some(Path::new("/x/angzarr-build")), Some("abc123")),
            "{ path = \"/x/angzarr-build\" }"
        );
        assert_eq!(
            angzarr_build_dependency(None, Some("0123456789ab")),
            format!("{{ git = \"{git}\", rev = \"0123456789ab\" }}")
        );
        for commit in [None, Some(""), Some("0123456789ab-dirty")] {
            assert_eq!(
                angzarr_build_dependency(None, commit),
                format!("{{ git = \"{git}\" }}"),
                "{commit:?}"
            );
        }
    }
}
