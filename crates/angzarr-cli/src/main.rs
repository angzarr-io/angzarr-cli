//! `angzarr`: the command line for angzarr projects.

use std::fmt;
use std::path::PathBuf;
use std::process::ExitCode;

use angzarr_build::{
    BuildError, Existing, ProtoInput, PythonApp, Scaffolded, StartupApp, Translate,
};
use angzarr_fmt::{Catalog, Failure, Format, Viewable};
use angzarr_fmt_clap::{FormatArgs, finish};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;

use angzarr_embedded_diagnostic::{Arg, Diagnostic, Report};

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
    /// What to check: `startup`, the host config.
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
    Startup,
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
    startup: StartupArgs,
}

/// What `scaffold startup` alone takes.
#[derive(Args)]
struct StartupArgs {
    /// Startup: the application's name, for the generated prose.
    #[arg(long, default_value = "the application")]
    name: String,
    /// Startup: a domain served by a Python child process; every other
    /// domain is Rust, in process. Repeatable.
    #[arg(long = "python", value_name = "DOMAIN")]
    python: Vec<String>,
    /// Startup: the application's build dependency on angzarr-build, a TOML
    /// value; unset, the angzarr-cli commit this CLI was built from.
    #[arg(long, value_name = "TOML")]
    angzarr_build: Option<String>,
}

/// What `scaffold python` alone takes.
#[derive(Args)]
struct PythonArgs {
    /// Python: the domain the project serves, e.g. `player`.
    #[arg(long)]
    domain: Option<String>,
    /// Python: the git repository `buf generate` reads the protos from;
    /// unset, it reads them from the first `--include`.
    #[arg(long, value_name = "URL", requires = "proto_ref")]
    proto_git: Option<String>,
    /// Python: the revision of `--proto-git`.
    #[arg(long, value_name = "REV")]
    proto_ref: Option<String>,
    /// Python: the protos' directory within `--proto-git`.
    #[arg(long, value_name = "DIR", default_value = "proto")]
    proto_subdir: String,
    /// Python: the build version the domain reports as its rules_version.
    #[arg(long, default_value = "0.1.0")]
    rules_version: String,
    /// Python: skip `buf generate` (and the rerooting of the message types'
    /// imports), which otherwise ends every run.
    #[arg(long)]
    no_proto_gen: bool,
    /// Python: skip `uv lock`, which otherwise ends every run.
    #[arg(long)]
    no_lock: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Lang {
    Rust,
    Python,
    /// The application's startup: its host config, build.rs and main.rs.
    Startup,
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
                    subdir: python.proto_subdir.clone(),
                },
                _ => ProtoInput::Directory(includes.first().copied().unwrap_or(".").to_string()),
            };
            let app = PythonApp {
                domain,
                protos: protos_input,
                rules_version: python.rules_version.clone(),
            };
            let done =
                angzarr_build::scaffold_python_into(&args.src, &protos, &includes, &app, existing)
                    .map_err(CliError::Build)?;
            if !python.no_proto_gen {
                angzarr_build::proto_gen_python(&args.src, &app).map_err(CliError::Build)?;
            }
            if !python.no_lock {
                angzarr_build::lock_python(&args.src).map_err(CliError::Build)?;
            }
            done
        }
        Lang::Startup => {
            let app = StartupApp {
                name: args.startup.name.clone(),
                python: args.startup.python.clone(),
                includes: args.includes.clone(),
                angzarr_build: args.startup.angzarr_build.clone().unwrap_or_else(|| {
                    format!(
                        "{{ git = \"https://github.com/angzarr-io/angzarr-cli\", rev = \"{}\" }}",
                        option_env!("ANGZARR_REV").unwrap_or("main")
                    )
                }),
            };
            angzarr_build::startup_into(&args.src, &protos, &includes, &app)
                .map_err(CliError::Build)?
        }
    };
    Ok(report(done))
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

/// `angzarr lint startup`: each lint a warning on stderr, and a failure
/// when there is any.
fn lint(args: LintArgs, format: Format, catalog: &Catalog) -> Result<LintReport, Box<dyn Failure>> {
    let LintWhat::Startup = args.what;
    let protos: Vec<&str> = args.protos.iter().map(String::as_str).collect();
    let includes: Vec<&str> = args.includes.iter().map(String::as_str).collect();
    let lints =
        angzarr_build::lint_startup(&args.config, &protos, &includes).map_err(CliError::Build)?;
    let mut stderr = std::io::stderr().lock();
    for lint in &lints {
        let warning = angzarr_fmt::Warning {
            prog: "angzarr",
            diagnostic: lint,
            remedy: None,
        };
        let _ = angzarr_fmt::write_warning(&mut stderr, format, catalog, &warning);
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
            CliError::Lints { .. } => "angzarr-cli-lints",
            CliError::Build(error) => error.code(),
        }
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        match self {
            CliError::TranslateMalformed { value } => vec![("value", value.as_str().into())],
            CliError::DomainRequired => Vec::new(),
            CliError::Lints { config, count } => vec![
                ("config", config.as_str().into()),
                ("count", (*count as i64).into()),
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
            CliError::Lints { .. } | CliError::Build(_) => None,
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
            let result = lint(args, format, &catalog);
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
}
