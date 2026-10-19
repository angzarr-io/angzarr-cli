//! Adapters for the angzarr CLI's ports.
//!
//! [`ImageSource`] is a [`DescriptorSource`] reading a compiled descriptor
//! set from a file or from stdin: a `FileDescriptorSet` (`protoc
//! --include_imports --descriptor_set_out`) or a binary buf image (`buf
//! build -o`). A buf image marks the files present only as imports; every
//! other file, and every file of a plain `FileDescriptorSet`, counts as
//! generated in this run.
//!
//! [`project`] reads and writes a project's `.angzarr/` settings:
//! [`DirProject`] on disk, [`MemoryProject`] in memory.

pub mod project;

pub use project::{DirProject, MemoryProject};

use std::collections::BTreeSet;
use std::fmt;
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use angzarr_cli_app::DescriptorSource;
use angzarr_cli_core::DescriptorSet;
use angzarr_fmt::{Failure, io_kind};
use annals_diagnostic::{Arg, Diagnostic, Report};
use prost::Message;
use prost_reflect::DescriptorPool;

/// This crate's en-US messages.
pub const EN_US: &str = include_str!("../locales/en-US/angzarr-cli-adapters.ftl");

/// What the CLI's image argument names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Path(PathBuf),
    /// `-`: the image is read from stdin.
    Stdin,
}

impl Input {
    /// `-` is stdin; anything else is a path.
    pub fn parse(arg: &str) -> Input {
        if arg == "-" {
            Input::Stdin
        } else {
            Input::Path(PathBuf::from(arg))
        }
    }

    /// How the input is named in a diagnostic: the path, or `-`.
    pub fn name(&self) -> String {
        match self {
            Input::Path(p) => p.display().to_string(),
            Input::Stdin => "-".to_string(),
        }
    }
}

/// Why an image could not be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceError {
    /// The input could not be read. `kind` is the I/O error kind as a
    /// kebab-case token (`not-found`).
    Read { input: String, kind: &'static str },
    /// No image was named and stdin is a terminal, so there is nothing to
    /// read.
    StdinTerminal,
    /// The bytes are not a descriptor set, or the set is not valid.
    /// `detail` is the decoder's own English text.
    Decode { input: String, detail: String },
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::Read { input, kind } => write!(f, "{input} could not be read: {kind}"),
            SourceError::StdinTerminal => f.write_str("no image was given and stdin is a terminal"),
            SourceError::Decode { input, detail } => {
                write!(f, "{input} is not a valid descriptor set: {detail}")
            }
        }
    }
}

impl std::error::Error for SourceError {}

impl Diagnostic for SourceError {
    fn code(&self) -> &'static str {
        match self {
            SourceError::Read { .. } => "angzarr-cli-adapters-read",
            SourceError::StdinTerminal => "angzarr-cli-adapters-stdin-terminal",
            SourceError::Decode { .. } => "angzarr-cli-adapters-decode",
        }
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        match self {
            SourceError::Read { input, kind } => {
                vec![("input", input.as_str().into()), ("kind", (*kind).into())]
            }
            SourceError::StdinTerminal => Vec::new(),
            SourceError::Decode { input, detail } => vec![
                ("input", input.as_str().into()),
                ("detail", detail.as_str().into()),
            ],
        }
    }
}

impl Failure for SourceError {
    fn remedy(&self) -> Option<Report> {
        match self {
            SourceError::Read { .. } => None,
            SourceError::StdinTerminal | SourceError::Decode { .. } => Some(Report::new(
                "angzarr-cli-adapters-fix-image",
                "pass a binary descriptor set or buf image, as a path or on stdin: buf build -o image.binpb, or protoc --include_imports --descriptor_set_out=image.binpb",
            )),
        }
    }
}

/// One of every diagnostic this crate raises, for the catalog check.
pub fn diagnostic_samples() -> Vec<Box<dyn Diagnostic>> {
    let decode = SourceError::Decode {
        input: "x".into(),
        detail: "x".into(),
    };
    let remedy = decode.remedy().expect("a decode failure has a remedy");
    let mut samples: Vec<Box<dyn Diagnostic>> = vec![
        Box::new(SourceError::Read {
            input: "x".into(),
            kind: "not-found",
        }),
        Box::new(SourceError::StdinTerminal),
        Box::new(decode),
        Box::new(remedy),
    ];
    samples.extend(project::samples());
    samples
}

/// A buf image file's own extension (`buf.alpha.image.v1.ImageFileExtension`).
#[derive(Clone, PartialEq, Message)]
struct ImageFileExtension {
    #[prost(bool, optional, tag = "1")]
    is_import: Option<bool>,
}

/// The parts of a descriptor-set file this adapter reads itself: its name
/// and buf's extension. Everything else is the pool's.
#[derive(Clone, PartialEq, Message)]
struct ImageFile {
    #[prost(string, optional, tag = "1")]
    name: Option<String>,
    #[prost(message, optional, tag = "8042")]
    buf: Option<ImageFileExtension>,
}

#[derive(Clone, PartialEq, Message)]
struct Image {
    #[prost(message, repeated, tag = "1")]
    file: Vec<ImageFile>,
}

/// Decodes a `FileDescriptorSet` or binary buf image. `input` names it in a
/// failure.
pub fn decode(bytes: &[u8], input: &str) -> Result<DescriptorSet, SourceError> {
    let failed = |detail: String| SourceError::Decode {
        input: input.to_string(),
        detail,
    };
    let image = Image::decode(bytes).map_err(|e| failed(e.to_string()))?;
    let pool = DescriptorPool::decode(bytes).map_err(|e| failed(e.to_string()))?;
    let generated: BTreeSet<String> = image
        .file
        .into_iter()
        .filter(|f| !f.buf.as_ref().and_then(|b| b.is_import).unwrap_or(false))
        .filter_map(|f| f.name)
        .collect();
    Ok(DescriptorSet { pool, generated })
}

/// A descriptor set read from a file or stdin.
#[derive(Clone, Debug)]
pub struct ImageSource {
    pub input: Input,
}

impl ImageSource {
    pub fn new(input: Input) -> ImageSource {
        ImageSource { input }
    }

    fn read(&self) -> Result<Vec<u8>, SourceError> {
        let read_failed = |e: std::io::Error| SourceError::Read {
            input: self.input.name(),
            kind: io_kind(e.kind()),
        };
        match &self.input {
            Input::Path(path) => std::fs::read(path).map_err(read_failed),
            Input::Stdin => {
                let stdin = std::io::stdin();
                if stdin.is_terminal() {
                    return Err(SourceError::StdinTerminal);
                }
                let mut bytes = Vec::new();
                stdin.lock().read_to_end(&mut bytes).map_err(read_failed)?;
                Ok(bytes)
            }
        }
    }
}

impl DescriptorSource for ImageSource {
    fn load(&self) -> Result<DescriptorSet, Box<dyn Failure>> {
        let bytes = self.read()?;
        Ok(decode(&bytes, &self.input.name())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use angzarr_cli_core::fixture::{Image as Fixture, counter};

    #[test]
    fn the_catalog_has_every_code_with_exactly_its_args() {
        let problems = angzarr_fmt::check(EN_US, "angzarr-cli-adapters", &diagnostic_samples());
        assert!(problems.is_empty(), "{problems:#?}");
    }

    #[test]
    fn dash_is_stdin_and_anything_else_a_path() {
        assert_eq!(Input::parse("-"), Input::Stdin);
        assert_eq!(Input::parse("a.binpb"), Input::Path("a.binpb".into()));
        assert_eq!(Input::Stdin.name(), "-");
    }

    #[test]
    fn a_buf_image_marks_its_imports_as_not_generated() {
        let bytes = Fixture::new().import(counter()).encode();
        let set = decode(&bytes, "x").unwrap();
        assert!(set.generated.is_empty(), "{:?}", set.generated);
        assert!(set.pool.get_message_by_name("c.v1.CounterState").is_some());

        let bytes = Fixture::new().file(counter()).encode();
        let set = decode(&bytes, "x").unwrap();
        assert_eq!(set.generated, BTreeSet::from(["counter.proto".to_string()]));
    }

    #[test]
    fn bytes_that_are_not_a_descriptor_set_fail_to_decode_naming_the_input() {
        let err = decode(&[0xff, 0xff, 0xff], "img.binpb").unwrap_err();
        assert!(
            matches!(&err, SourceError::Decode { input, .. } if input == "img.binpb"),
            "{err:?}"
        );
        assert_eq!(err.code(), "angzarr-cli-adapters-decode");
        assert!(err.remedy().is_some());
    }

    #[test]
    fn a_missing_file_is_a_read_failure_with_its_kind() {
        let source = ImageSource::new(Input::parse("/nonexistent/angzarr/image.binpb"));
        let err = source.load().unwrap_err();
        assert_eq!(err.code(), "angzarr-cli-adapters-read");
        let args = err.args();
        assert!(
            args.iter()
                .any(|(n, a)| *n == "kind" && matches!(a, Arg::Str(s) if s == "not-found"))
        );
    }
}
