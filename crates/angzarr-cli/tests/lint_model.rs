//! `angzarr lint` and `angzarr model` end to end, on fixture images written
//! to a temporary file or piped to stdin. Stdout is piped, so the default
//! format is json.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use angzarr_cli_core::fixture::{
    ComponentKind, ConflictResolution, File, Fld, Image, Msg, component, counter, state, ty,
};
use serde_json::Value;

fn write_image(name: &str, image: &Image) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, image.encode()).unwrap();
    path
}

fn angzarr(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_angzarr"))
        .args(args)
        .output()
        .unwrap()
}

fn angzarr_stdin(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_angzarr"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or_else(|e| {
        panic!("{e}: {}", String::from_utf8_lossy(bytes));
    })
}

fn missing_domain() -> Image {
    Image::new().file(
        File::new("k.proto", "k.v1").message(Msg::new("A").component(component(
            ComponentKind::Aggregate,
            "",
            "",
            &[],
        ))),
    )
}

#[test]
fn a_clean_image_lints_to_no_diagnostics_and_exits_0() {
    let path = write_image("clean.binpb", &Image::new().file(counter()));
    let out = angzarr(&["lint", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(json(&out.stdout), serde_json::json!({"diagnostics": []}));
}

#[test]
fn an_error_finding_is_written_and_exits_1() {
    let path = write_image("missing-domain.binpb", &missing_domain());
    let out = angzarr(&["lint", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let value = json(&out.stdout);
    let d = &value["diagnostics"][0];
    assert_eq!(d["code"], "angzarr-cli-core-missing-field");
    assert_eq!(d["severity"], "error");
    assert_eq!(d["location"], "k.proto#k.v1.A");
    assert_eq!(
        d["message"],
        "k.v1.A: an aggregate requires (component).domain"
    );
    assert_eq!(d["remedy"]["code"], "angzarr-cli-core-fix-set-field");
    assert!(out.stderr.is_empty());
}

#[test]
fn warnings_alone_exit_0() {
    let file = File::new("s.proto", "s.v1").message(
        Msg::new("S")
            .component(component(ComponentKind::Aggregate, "s", "", &[]))
            .state(state(&[ConflictResolution::Commutative]))
            .field(Fld::scalar("a", 1, ty::STRING).conflict(Default::default()))
            .field(Fld::scalar("b", 2, ty::STRING)),
    );
    let out = angzarr_stdin(&["lint", "-"], &Image::new().file(file).encode());
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let value = json(&out.stdout);
    assert_eq!(value["diagnostics"][0]["severity"], "warning");
    assert_eq!(
        value["diagnostics"][0]["code"],
        "angzarr-cli-core-unmarked-field"
    );
}

#[test]
fn lint_reads_stdin_and_renders_any_format() {
    let out = angzarr_stdin(
        &["lint", "-", "--format", "toml"],
        &missing_domain().encode(),
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("[[diagnostics]]\n"), "{text}");

    let out = angzarr_stdin(&["lint", "--format", "text"], &missing_domain().encode());
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("set (component).domain"), "{text}");
}

#[test]
fn model_prints_the_component_model() {
    let path = write_image("model.binpb", &Image::new().file(counter()));
    let out = angzarr(&["model", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let value = json(&out.stdout);
    let c = &value["components"][0];
    assert_eq!(c["anchor"], "c.v1.CounterState");
    assert_eq!(c["kind"], "aggregate");
    assert_eq!(c["domain"], "counter");
    assert_eq!(c["commands"][0]["type"], "c.v1.Increment");
    assert_eq!(c["events"][0]["applies"], true);

    let out = angzarr(&["model", path.to_str().unwrap(), "--format", "text"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.starts_with("CounterState:\n  Kind: aggregate\n"),
        "{text}"
    );
}

#[test]
fn an_unreadable_image_fails_on_stderr_with_its_code() {
    let out = angzarr(&["lint", "/nonexistent/angzarr/image.binpb"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let value = json(&out.stderr);
    assert_eq!(value["error"]["code"], "angzarr-cli-adapters-read");
    assert_eq!(value["error"]["args"]["kind"], "not-found");
}

#[test]
fn bytes_that_are_not_an_image_fail_with_a_remedy() {
    let out = angzarr_stdin(&["model", "-"], &[0xff, 0xff, 0xff]);
    assert_eq!(out.status.code(), Some(1));
    let value = json(&out.stderr);
    assert_eq!(value["error"]["code"], "angzarr-cli-adapters-decode");
    assert_eq!(value["remedy"]["code"], "angzarr-cli-adapters-fix-image");
}
