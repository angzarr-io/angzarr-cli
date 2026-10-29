//! `angzarr scaffold python`: one domain's uv project, the user's files
//! written once and completed with `--missing`, the CLI's rewritten on
//! every run.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const PLAYER: &str = "io/angzarr/examples/v1/player.proto";
const IMPL: &str = "src/angzarr_player/logic_impl.py";
const MAIN: &str = "src/angzarr_player/main.py";

fn fixtures() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../angzarr-codegen/tests/fixtures")
        .display()
        .to_string()
}

fn golden(path: &str) -> String {
    let golden = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../angzarr-codegen/tests/golden/python/player")
        .join(path);
    std::fs::read_to_string(&golden).unwrap_or_else(|_| panic!("{}", golden.display()))
}

fn empty_dir() -> PathBuf {
    static RUN: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "angzarr-cli-scaffold-python-test-{}-{}",
        std::process::id(),
        RUN.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `angzarr scaffold python` for the player into `root`, offline (no buf,
/// no uv), with `extra`.
fn scaffold(root: &Path, extra: &[&str]) -> Output {
    online(root, &[&["--no-proto-gen", "--no-lock"], extra].concat())
}

/// `angzarr scaffold python` for the player into `root` as a user runs it,
/// with `extra`.
fn online(root: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_angzarr"))
        .args(["scaffold", "python", "--format", "json", "-I", &fixtures()])
        .args(["--domain", "player"])
        .args([
            "--proto-git",
            "https://github.com/angzarr-io/angzarr-project",
            "--proto-ref",
            "a91b88262921786c246e63e2e694509fe8060a86",
        ])
        .arg("--src")
        .arg(root)
        .args(extra)
        .arg(PLAYER)
        .output()
        .unwrap()
}

/// The action taken on each file, by its path relative to `root`.
fn actions(out: &Output, root: &Path) -> Vec<(String, String)> {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let mut actions: Vec<(String, String)> = value["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| {
            let path = PathBuf::from(f["path"].as_str().unwrap());
            (
                path.strip_prefix(root).unwrap().display().to_string(),
                f["action"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    actions.sort();
    actions
}

fn action_of<'a>(actions: &'a [(String, String)], path: &str) -> &'a str {
    &actions
        .iter()
        .find(|(p, _)| p == path)
        .unwrap_or_else(|| panic!("no {path} in {actions:?}"))
        .1
}

#[test]
fn scaffold_python_writes_the_golden_project() {
    let root = empty_dir().join("player");
    let done = actions(&scaffold(&root, &[]), &root);
    assert!(
        done.iter().all(|(_, action)| action == "created"),
        "{done:?}"
    );
    for (path, _) in &done {
        assert_eq!(
            std::fs::read_to_string(root.join(path)).unwrap(),
            golden(path),
            "{path}"
        );
    }
    assert_eq!(done.len(), 16, "{done:?}");
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn scaffold_python_rewrites_its_own_files_and_never_the_users() {
    let root = empty_dir();
    scaffold(&root, &[]);
    let mine = "\"\"\"Mine now.\"\"\"\n\n\nclass PlayerLogicImpl:\n    pass\n";
    std::fs::write(root.join(IMPL), mine).unwrap();
    std::fs::write(root.join(MAIN), "# edited by hand\n").unwrap();
    let done = actions(&scaffold(&root, &[]), &root);
    assert_eq!(action_of(&done, IMPL), "kept");
    assert_eq!(action_of(&done, MAIN), "regenerated");
    assert_eq!(std::fs::read_to_string(root.join(IMPL)).unwrap(), mine);
    assert_eq!(
        std::fs::read_to_string(root.join(MAIN)).unwrap(),
        golden(MAIN)
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn scaffold_python_missing_adds_the_absent_methods_only() {
    let root = empty_dir();
    scaffold(&root, &[]);
    let fresh = std::fs::read_to_string(root.join(IMPL)).unwrap();
    let applier = "\n    def apply_move_decided(self, state: PlayerState, event: MoveDecided) -> PlayerState:\n        raise NotImplementedError  # TODO: fold MoveDecided into the state\n";
    let mine = fresh.replace(applier, "").replace(
        "raise NotImplementedError  # TODO: decide DecideMove",
        "return MoveDecided()  # mine",
    );
    assert_ne!(mine, fresh);
    std::fs::write(root.join(IMPL), &mine).unwrap();
    let done = actions(&scaffold(&root, &["--missing"]), &root);
    assert_eq!(action_of(&done, IMPL), "completed");
    assert_eq!(
        std::fs::read_to_string(root.join(IMPL)).unwrap(),
        format!("{mine}{applier}")
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn scaffold_python_missing_refuses_a_command_taken_as_a_fact() {
    let root = empty_dir();
    scaffold(&root, &[]);
    let mine = std::fs::read_to_string(root.join(IMPL))
        .unwrap()
        .replace("cmd: DecideMove", "fact: DecideMove");
    std::fs::write(root.join(IMPL), &mine).unwrap();
    let out = scaffold(&root, &["--missing"]);
    assert!(!out.status.success());
    let failure: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert!(
        failure
            .to_string()
            .contains("angzarr-codegen-input-mismatch"),
        "{failure}"
    );
    assert_eq!(std::fs::read_to_string(root.join(IMPL)).unwrap(), mine);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn scaffold_python_needs_a_domain() {
    let root = empty_dir();
    let out = Command::new(env!("CARGO_BIN_EXE_angzarr"))
        .args(["scaffold", "python", "--format", "json", "-I", &fixtures()])
        .arg("--src")
        .arg(&root)
        .arg(PLAYER)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let failure: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert!(
        failure.to_string().contains("angzarr-cli-domain-required"),
        "{failure}"
    );
    assert!(std::fs::read_dir(&root).unwrap().next().is_none());
    std::fs::remove_dir_all(&root).unwrap();
}

/// The default run, as a user runs it, ends with the project ready for
/// `uv run --locked`: its message types generated (buf's remote plugins)
/// and rerooted, its `uv.lock` written, and the child importable, building
/// its host.
#[test]
#[ignore = "needs the network (buf.build, GitHub) and cargo (angzarr-client builds its \
            router library): run with --ignored"]
fn scaffold_python_by_default_generates_the_wire_types_and_the_lock_and_runs_under_uv() {
    let root = empty_dir();
    let done = actions(&online(&root, &[]), &root);
    assert_eq!(action_of(&done, MAIN), "created");
    let pb2 = std::fs::read_to_string(
        root.join("src/angzarr_player/_gen/io/angzarr/examples/v1/player_pb2.py"),
    )
    .unwrap();
    assert!(
        pb2.contains("from angzarr_player._gen.io.angzarr.examples.v1 import cards_pb2"),
        "{pb2}"
    );
    assert!(root.join("uv.lock").exists());
    assert!(root.join("src/angzarr_player/_version.py").exists());
    let out = Command::new("uv")
        .args(["run", "--locked", "python", "-c"])
        .arg("from angzarr_player.main import host; host()")
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_dir_all(&root).unwrap();
}
