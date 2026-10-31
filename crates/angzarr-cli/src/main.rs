//! `angzarr`: the command line for angzarr projects.

use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use angzarr_build::{
    AppSeed, BuildError, DomainCrate, Existing, Exposure, ProtoInput, PythonApp, RetrySeed,
    Scaffolded, Translate,
};
use angzarr_fmt::{Catalog, Failure, Format, Viewable};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;

use angzarr_embedded_diagnostic::{Arg, Diagnostic, Report};

mod output;

use output::{FormatArgs, finish};

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
    #[arg(long, default_value = "angzarr.toml")]
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
    /// The refusal switch, set to translate for one command of one policy:
    /// `<policy>:<command>=<fact>`, each fully qualified. Every other command
    /// is passed through.
    #[arg(long, value_name = "POLICY:COMMAND=FACT")]
    translate: Vec<String>,
    /// Add the stubs an existing file lacks, changing nothing else.
    #[arg(long)]
    missing: bool,
    #[command(flatten)]
    python: PythonArgs,
    #[command(flatten)]
    app: AppArgs,
}

/// What `scaffold app` alone takes: the seed of a new host config, unused
/// once `angzarr.toml` exists (it is the user's).
#[derive(Args)]
struct AppArgs {
    /// App: the application's name, for the generated prose.
    #[arg(long, default_value = "the application")]
    name: String,
    /// App: a domain served by a Python child process; every other domain
    /// is Rust, in process. Repeatable.
    #[arg(long = "python", value_name = "DOMAIN")]
    python: Vec<String>,
    /// App: a domain exposed through the host's listener,
    /// `<domain>=loopback` or `<domain>=ip`. Repeatable.
    #[arg(long, value_name = "DOMAIN=EXPOSURE")]
    expose: Vec<String>,
    /// App: `[retry] max_retries`; unset, the host's default.
    #[arg(long, value_name = "N")]
    max_retries: Option<u32>,
    /// App: `[retry] max_race_runs`; 0 hands a lost race back as the
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
    /// `created`, `completed`, `regenerated` or `kept`.
    action: &'static str,
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
    let done = match args.lang {
        Lang::Rust => {
            let translate = args
                .translate
                .iter()
                .map(String::as_str)
                .map(translate)
                .collect::<Result<Vec<_>, _>>()?;
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
            let seed = AppSeed {
                name: args.app.name.clone(),
                includes: args.includes.clone(),
                python: args.app.python.clone(),
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

/// One entry per file, by what was done with it.
fn report(done: Vec<Scaffolded>) -> ScaffoldReport {
    let files = done
        .into_iter()
        .map(|d| {
            let (path, action) = match d {
                Scaffolded::Created(path) => (path, "created"),
                Scaffolded::Completed(path) => (path, "completed"),
                Scaffolded::Regenerated(path) => (path, "regenerated"),
                Scaffolded::Kept(path) => (path, "kept"),
            };
            ScaffoldEntry {
                path: path.display().to_string(),
                action,
            }
        })
        .collect();
    ScaffoldReport { files }
}

/// `angzarr lint app`: each lint a warning on stderr, and a failure when
/// there is any.
fn lint(args: LintArgs, catalog: &Catalog) -> Result<LintReport, Box<dyn Failure>> {
    let LintWhat::App = args.what;
    let protos: Vec<&str> = args.protos.iter().map(String::as_str).collect();
    let includes: Vec<&str> = args.includes.iter().map(String::as_str).collect();
    let lints =
        angzarr_build::lint_app(&args.config, &protos, &includes).map_err(CliError::Build)?;
    // angzarr-fmt has no warning of its own: each lint is one line, its
    // code and its message.
    for lint in &lints {
        eprintln!(
            "angzarr: warning[{}]: {}",
            lint.code(),
            catalog.render(lint)
        );
    }
    if !lints.is_empty() {
        return Err(CliError::Lints {
            config: args.config.display().to_string(),
            count: lints.len(),
        }
        .into());
    }
    Ok(LintReport {
        config: args.config.display().to_string(),
        lints: 0,
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
    /// `lint` found `count` problems in `config`.
    Lints { config: String, count: usize },
    /// Generation failed; reported as the build error itself.
    Build(BuildError),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::TranslateMalformed { value } => {
                write!(f, "--translate {value} is not <policy>:<command>=<fact>")
            }
            CliError::DomainRequired => f.write_str("scaffold python needs --domain"),
            CliError::ExposeMalformed { value } => {
                write!(
                    f,
                    "--expose {value} is not <domain>=loopback or <domain>=ip"
                )
            }
            CliError::Lints { config, count } => write!(f, "{config} has {count} lints"),
            CliError::Build(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for CliError {}

impl Diagnostic for CliError {
    fn code(&self) -> &'static str {
        match self {
            CliError::TranslateMalformed { .. } => "angzarr-cli-translate-malformed",
            CliError::DomainRequired => "angzarr-cli-domain-required",
            CliError::ExposeMalformed { .. } => "angzarr-cli-expose-malformed",
            CliError::Lints { .. } => "angzarr-cli-lints",
            CliError::Build(error) => error.code(),
        }
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        match self {
            CliError::TranslateMalformed { value } => vec![("value", value.as_str().into())],
            CliError::DomainRequired => Vec::new(),
            CliError::ExposeMalformed { value } => vec![("value", value.as_str().into())],
            CliError::Lints { config, count } => vec![
                ("config", config.as_str().into()),
                ("count", i64::try_from(*count).unwrap_or(i64::MAX).into()),
            ],
            CliError::Build(error) => error.args(),
        }
    }
}

impl Failure for CliError {
    fn remedy(&self) -> Option<Report> {
        match self {
            CliError::TranslateMalformed { .. } => Some(Report::new("angzarr-cli-translate-form")),
            CliError::DomainRequired => Some(Report::new("angzarr-cli-domain-name")),
            CliError::ExposeMalformed { .. } | CliError::Lints { .. } | CliError::Build(_) => None,
        }
    }
}

impl From<CliError> for Box<dyn Failure> {
    fn from(error: CliError) -> Self {
        Box::new(error)
    }
}

/// This crate's en-US messages.
const EN_US: &str = include_str!("../locales/en-US/angzarr-cli.ftl");

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
        angzarr_fmt::EN_US,
        angzarr_codegen::EN_US,
        angzarr_build::EN_US,
        EN_US,
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
        vec![
            Box::new(malformed),
            Box::new(remedy),
            Box::new(domain),
            Box::new(domain_remedy),
            Box::new(CliError::ExposeMalformed {
                value: "x".to_string(),
            }),
            Box::new(CliError::Lints {
                config: "x".to_string(),
                count: 2,
            }),
        ]
    }

    #[test]
    fn every_error_has_a_catalog_message_with_its_args() {
        let problems = angzarr_fmt::check(EN_US, &diagnostic_samples());
        assert!(problems.is_empty(), "{problems:#?}");
    }

    #[test]
    fn every_errors_english_is_its_catalog_message() {
        let catalog = catalog();
        let errors = [
            CliError::TranslateMalformed {
                value: "x".to_string(),
            },
            CliError::DomainRequired,
            CliError::ExposeMalformed {
                value: "x".to_string(),
            },
            CliError::Lints {
                config: "x".to_string(),
                count: 2,
            },
            CliError::Build(BuildError::Uv {
                detail: "x".to_string(),
            }),
        ];
        for sample in errors {
            let rendered: String = catalog
                .render(&sample)
                .chars()
                .filter(|c| !matches!(c, '\u{2068}' | '\u{2069}'))
                .collect();
            assert_eq!(sample.to_string(), rendered, "{}", sample.code());
        }
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
