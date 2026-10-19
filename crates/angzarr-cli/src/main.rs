//! `angzarr`: the command line for angzarr projects.

use std::process::ExitCode;

use angzarr_cli_adapters::{ImageSource, Input};
use angzarr_fmt::{Catalog, FmtError, Format, Viewable};
use angzarr_fmt_clap::{FormatArgs, Outcome};
use clap::{Parser, Subcommand};
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
    /// Check the angzarr declarations in a compiled descriptor set; exits 1
    /// when any finding is an error.
    Lint {
        /// A binary FileDescriptorSet or buf image, or - for stdin.
        #[arg(default_value = "-")]
        image: String,
    },
    /// Show the component model a compiled descriptor set declares.
    Model {
        /// A binary FileDescriptorSet or buf image, or - for stdin.
        #[arg(default_value = "-")]
        image: String,
    },
}

/// Which build of the CLI this is.
#[derive(Serialize)]
struct Version {
    name: &'static str,
    version: &'static str,
    /// The short hash of the commit it was built from, when there was one.
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
    for ftl in [
        angzarr_cli_core::EN_US,
        angzarr_cli_app::EN_US,
        angzarr_cli_adapters::EN_US,
    ] {
        catalog
            .add(ftl)
            .expect("each crate's catalog parses; its own tests check it");
    }
    catalog
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let catalog = catalog();
    let reporter = cli.format.reporter(Format::Json, &catalog, "angzarr");
    match cli.command {
        Command::Version => reporter.finish(Ok::<_, FmtError>(Outcome::ok(version()))),
        Command::Lint { image } => {
            let source = ImageSource::new(Input::parse(&image));
            reporter.finish(angzarr_cli_app::lint(&source).map(|report| {
                let status = u8::from(report.has_errors());
                Outcome::with_status(report, status)
            }))
        }
        Command::Model { image } => {
            let source = ImageSource::new(Input::parse(&image));
            reporter.finish(angzarr_cli_app::model(&source).map(Outcome::ok))
        }
    }
}
