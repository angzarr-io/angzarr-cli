//! `angzarr probe <component>` end to end: the binary reads the host
//! config, takes the component through the child protocol with
//! angzarr-embedded-host's probe, and prints each step's outcome, a failed
//! one as its code, its args and its English; any failed step fails the
//! command.
//!
//! The failure modes run against fake children that are shell commands,
//! each breaking the protocol at once, so no test waits for a time limit:
//! one that cannot start, one that writes a malformed handshake, one whose
//! stdout ends first. The child that passes every step is client-python's
//! shim, as the Python player runs it (ignored: it needs uv and the
//! player's checkout).

use std::path::Path;
use std::process::{Command, Output};

#[path = "../../../test-support/test_dir.rs"]
mod test_dir;
use test_dir::TestDir;

use angzarr_diagnostic::{Diagnostic, code};
use angzarr_embedded_host::{HandshakeError, HostError};

/// The host's failure to start a child whose handshake failed: its code is
/// the host's, whatever the cause.
fn handshake() -> HostError {
    HostError::Handshake {
        domain: String::new(),
        cause: HandshakeError::Closed,
    }
}

/// `angzarr probe <args>` in `dir`.
fn probe(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_angzarr"))
        .arg("probe")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

/// A directory holding `config` as its `angzarr.yaml`.
fn app(config: &str) -> TestDir {
    let dir = TestDir::new(concat!(env!("CARGO_PKG_NAME"), "-probe"));
    std::fs::write(dir.join("angzarr.yaml"), config).unwrap();
    dir
}

/// The config of one domain, `player`, launched by `launch` (a YAML
/// sequence).
fn launched(launch: &str) -> String {
    format!(
        "schema_version: 2\n\
         component:\n  \
           - name: player\n    \
             kind: domain\n    \
             language: python\n    \
             launch: {launch}\n"
    )
}

/// `out`'s stdout as JSON.
fn json(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// Each step's name and outcome, in order.
fn outcomes(report: &serde_json::Value) -> Vec<(String, String)> {
    report["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["step"].as_str().unwrap().to_string(),
                s["outcome"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// The six steps: `passed` passed, then one failed, then the rest not run.
fn failed_after(passed: usize) -> Vec<(String, String)> {
    [
        "launch",
        "handshake",
        "health",
        "describe",
        "handle",
        "shutdown",
    ]
    .iter()
    .enumerate()
    .map(|(n, step)| {
        let outcome = match n {
            n if n < passed => "passed",
            n if n == passed => "failed",
            _ => "not-run",
        };
        (step.to_string(), outcome.to_string())
    })
    .collect()
}

/// A launch command that cannot start fails the launch step: the host's
/// own failure, its English through the host's catalog, and every step
/// after it not run. The command fails.
#[test]
fn a_child_that_cannot_start_fails_the_launch_step() {
    let dir = app(&launched("[/nonexistent/angzarr-player]"));
    let out = probe(&dir, &["player", "--command", "/x.SitDown"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let report = json(&out);
    assert_eq!(report["component"], "player");
    assert_eq!(report["passed"], false);
    assert_eq!(outcomes(&report), failed_after(0));
    let failure = &report["steps"][0]["failure"];
    let spawn = HostError::Spawn {
        domain: String::new(),
        program: String::new(),
        kind: String::new(),
    };
    assert_eq!(failure["code"], spawn.code());
    assert_eq!(failure["args"]["program"], "/nonexistent/angzarr-player");
    assert_eq!(
        failure["message"],
        "component \"player\": could not start \"/nonexistent/angzarr-player\" (NotFound)"
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains(code!("probe-failed")), "{stderr}");
}

/// A malformed handshake fails the handshake step under the spec's key,
/// nested in the host's failure to start the child, each rendered in its
/// own catalog: the host's, and the spec's.
#[test]
fn a_malformed_handshake_fails_the_handshake_step_under_the_specs_key() {
    let dir = app(&launched(
        "[sh, -c, 'echo \"angzarr-handshake v2 unix:/tmp/x.sock\"; cat >/dev/null']",
    ));
    let out = probe(&dir, &["player", "--command", "/x.SitDown"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let report = json(&out);
    assert_eq!(outcomes(&report), failed_after(1));
    let failure = &report["steps"][1]["failure"];
    assert_eq!(failure["code"], handshake().code());
    let cause = &failure["args"]["cause"];
    assert_eq!(cause["code"], "HOST_REASON_HANDSHAKE_MALFORMED");
    assert_eq!(
        cause["args"]["line"],
        "angzarr-handshake v2 unix:/tmp/x.sock"
    );
    assert_eq!(
        failure["message"],
        "component \"player\": malformed handshake \"angzarr-handshake v2 unix:/tmp/x.sock\": \
         expected angzarr-handshake v1 unix:<absolute path>"
    );
}

/// A child whose stdout ends before its handshake fails the handshake
/// step, `HOST_REASON_HANDSHAKE_CLOSED`; the text view is a table of the
/// steps, each failure's English with its code.
#[test]
fn a_child_whose_stdout_ends_first_fails_the_handshake_step() {
    let dir = app(&launched("[sh, -c, 'exit 0']"));
    let out = probe(
        &dir,
        &["player", "--command", "/x.SitDown", "--format", "text"],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    assert_eq!(
        lines,
        [
            "Component: player",
            "Passed: false",
            "Step       Outcome  Detail",
            "launch     passed",
            &format!(
                "handshake  failed   component \"player\": stdout closed before a handshake \
                 [{}]",
                handshake().code()
            ),
            "health     not-run",
            "describe   not-run",
            "handle     not-run",
            "shutdown   not-run",
        ],
        "{text}"
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("component \"player\" failed the probe at its handshake step"),
        "{stderr}"
    );
}

/// A component the config does not list is refused before anything
/// starts, naming it; so is a domain or policy given no input.
#[test]
fn a_component_not_listed_or_given_no_input_is_refused() {
    let dir = app(&launched("[/nonexistent/angzarr-player]"));
    let out = probe(&dir, &["dealer", "--command", "/x.SitDown"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(out.stdout.is_empty());
    let failure: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(failure["error"]["code"], code!("probe-not-listed"));
    assert_eq!(failure["error"]["args"]["component"], "dealer");
    let out = probe(&dir, &["player"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let failure: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(failure["error"]["code"], code!("probe-no-input"));
    assert_eq!(failure["remedy"]["code"], code!("probe-input-form"));
}

/// A config that cannot be read is the host's own failure to read it.
#[test]
fn a_config_that_cannot_be_read_is_refused() {
    let dir = TestDir::new(concat!(env!("CARGO_PKG_NAME"), "-probe"));
    let out = probe(&dir, &["player", "--command", "/x.SitDown"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let failure: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    let unreadable = HostError::ConfigUnreadable {
        path: String::new(),
        kind: String::new(),
    };
    assert_eq!(failure["error"]["code"], unreadable.code());
    assert_eq!(
        failure["error"]["message"],
        "the config angzarr.yaml could not be read (NotFound)"
    );
}

/// client-python's child shim, as the Python player runs it by its default
/// launch (`uv run --locked angzarr-player`), passes every step, its
/// `DecideMove` answered, and the command succeeds. Ignored by default: it
/// needs uv and the player's checkout; `ANGZARR_PLAYER_DIR` names the
/// checkout's `player` directory.
#[test]
#[ignore = "needs uv and the Python player's checkout"]
fn client_pythons_child_shim_passes_every_step() {
    let player = std::env::var("ANGZARR_PLAYER_DIR").expect("ANGZARR_PLAYER_DIR names the player");
    let dir = app(&format!(
        "schema_version: 2\n\
         component:\n  \
           - name: player\n    \
             kind: domain\n    \
             language: python\n    \
             entry: [angzarr-player]\n    \
             dir: {player:?}\n\
         supervision:\n  \
           handshake_timeout_ms: 120000\n"
    ));
    let out = probe(
        &dir,
        &[
            "player",
            "--command",
            "type.googleapis.com/io.angzarr.examples.v1.DecideMove",
        ],
    );
    let report = json(&out);
    assert_eq!(out.status.code(), Some(0), "{report:#}");
    assert_eq!(report["passed"], true);
    assert!(
        outcomes(&report)
            .iter()
            .all(|(_, outcome)| outcome == "passed"),
        "{report:#}"
    );
}
