//! `angzarr`: the command line for angzarr projects.

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

use angzarr_cli_adapters::{DirProject, ImageSource, Input};
use angzarr_cli_app::{InitRequest, Project};
use angzarr_cli_core::project::NonTtyFormat;
use angzarr_fmt::{Catalog, Failure, FmtError, Format, Viewable};
use angzarr_fmt_clap::{FormatArgs, Outcome, Reporter};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use serde::Serialize;

/// The help text for `--format`, naming where the off-terminal default
/// comes from.
const FORMAT_HELP: &str = "Output format: json, yaml, toml, text or markdown [default: text on a terminal; otherwise output.non_tty_format in the nearest .angzarr/config.yaml, or json]";

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
    /// Create .angzarr/ in the current directory: config.yaml, lock.yaml,
    /// and a .gitignore keeping its cache/ out of version control.
    Init {
        /// A language to generate wiring for: rust or python. Repeat for
        /// each; none by default.
        #[arg(long = "target", value_name = "TARGET")]
        targets: Vec<String>,
        /// The angzarr-project commit the project builds against, in full.
        #[arg(long, value_name = "SHA")]
        spec_commit: String,
        /// Show what would be written, and write nothing.
        #[arg(long)]
        dry_run: bool,
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

/// The format a project's `non_tty_format` names.
fn format_of(f: NonTtyFormat) -> Format {
    f.token()
        .parse()
        .expect("every non_tty_format names a format; a test checks it")
}

/// The format results are written in: the flag when given, text when stdout
/// is a terminal, else the project's `non_tty_format`, else json.
/// `configured` is asked only in that last case.
fn resolve_format(
    flag: Option<Format>,
    stdout_terminal: bool,
    configured: impl FnOnce() -> Option<Format>,
) -> Format {
    match flag {
        Some(format) => format,
        None if stdout_terminal => Format::Text,
        None => configured().unwrap_or(Format::Json),
    }
}

/// The nearest project's `non_tty_format`, from the current directory up.
/// A config that does not load is `Err`: the command warns and goes on.
fn configured_format() -> Result<Option<Format>, Box<dyn Failure>> {
    let Some(project) = std::env::current_dir()
        .ok()
        .and_then(|cwd| DirProject::discover(&cwd))
    else {
        return Ok(None);
    };
    Ok(project.read_config()?.output.non_tty_format.map(format_of))
}

fn parse() -> Cli {
    let command = Cli::command().mut_arg("format", |arg| arg.help(FORMAT_HELP));
    let matches = command.get_matches();
    Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit())
}

fn main() -> ExitCode {
    let cli = parse();
    let catalog = catalog();
    let mut unreadable = None;
    let format = resolve_format(
        cli.format.format.map(|c| c.0),
        io::stdout().is_terminal(),
        || {
            configured_format().unwrap_or_else(|e| {
                unreadable = Some(e);
                None
            })
        },
    );
    let reporter = Reporter::new(format, cli.format.is_explicit(), &catalog, "angzarr");
    if let Some(failure) = unreadable {
        reporter.warn(&*failure, failure.remedy());
    }
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
        Command::Init {
            targets,
            spec_commit,
            dry_run,
        } => {
            let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let request = InitRequest {
                targets,
                spec_commit,
                dry_run,
            };
            reporter.finish(angzarr_cli_app::init(&DirProject::at(root), &request).map(Outcome::ok))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_wins_and_the_project_is_not_consulted() {
        for terminal in [false, true] {
            let format = resolve_format(Some(Format::Toml), terminal, || {
                panic!("the project is read only off a terminal with no flag")
            });
            assert_eq!(format, Format::Toml);
        }
    }

    #[test]
    fn a_terminal_gets_text_whatever_the_project_says() {
        let format = resolve_format(None, true, || {
            panic!("the project is read only off a terminal with no flag")
        });
        assert_eq!(format, Format::Text);
    }

    #[test]
    fn off_a_terminal_the_project_default_applies_else_json() {
        assert_eq!(
            resolve_format(None, false, || Some(Format::Markdown)),
            Format::Markdown
        );
        assert_eq!(resolve_format(None, false, || None), Format::Json);
    }

    #[test]
    fn every_non_tty_format_is_a_format_of_the_same_name() {
        let names: Vec<_> = NonTtyFormat::ALL
            .into_iter()
            .map(|f| format_of(f).name())
            .collect();
        let all: Vec<_> = Format::ALL.into_iter().map(Format::name).collect();
        assert_eq!(names, all);
    }

    #[test]
    fn the_format_help_is_replaced_on_the_global_flag() {
        let command = Cli::command().mut_arg("format", |arg| arg.help(FORMAT_HELP));
        command.clone().debug_assert();
        let arg = command
            .get_arguments()
            .find(|a| a.get_id() == "format")
            .unwrap();
        assert!(arg.is_global_set());
        assert_eq!(arg.get_help().unwrap().to_string(), FORMAT_HELP);
    }
}
