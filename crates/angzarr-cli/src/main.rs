//! `angzarr`: the command line for angzarr projects.

use std::path::PathBuf;
use std::process::ExitCode;

use angzarr_fmt::{Catalog, Failure, Format, Viewable};
use angzarr_fmt_clap::{FormatArgs, finish};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;

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
    Scaffold(ScaffoldArgs),
}

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
    /// The crate's source directory, where each component's file goes.
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
}

#[derive(Clone, Copy, ValueEnum)]
enum Lang {
    Rust,
}

/// What a scaffold run did, one entry per component file.
#[derive(Serialize)]
struct ScaffoldReport {
    files: Vec<ScaffoldEntry>,
}

#[derive(Serialize)]
struct ScaffoldEntry {
    path: String,
    /// `created`, `completed` or `kept`.
    action: &'static str,
}

impl Viewable for ScaffoldReport {}

fn scaffold(args: ScaffoldArgs) -> Result<ScaffoldReport, Box<dyn Failure>> {
    let _ = (
        args.lang,
        args.protos,
        args.includes,
        args.src,
        args.translate,
        args.missing,
    );
    Ok(ScaffoldReport { files: Vec::new() })
}

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
    catalog
        .add(angzarr_fmt::EN_US)
        .expect("angzarr-fmt's catalog parses; its own tests check it");
    catalog
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let format = cli.format.resolve(Format::Json);
    let catalog = catalog();
    match cli.command {
        Command::Version => finish(format, &catalog, Ok::<_, Box<dyn Failure>>(version())),
        Command::Scaffold(args) => finish(format, &catalog, scaffold(args)),
    }
}
