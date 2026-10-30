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
            "b9b87bfc30acbb90b089515761186ceb5aa6d52d",
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
    assert_eq!(done.len(), 17, "{done:?}");
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

/// A protos directory of its own, outside any project: the blackjack
/// protos (the fixtures') and angzarr's, as a user's checkout of the spec's
/// `proto` holds them.
fn proto_dir() -> PathBuf {
    let dir = empty_dir();
    let examples = "io/angzarr/examples/v1";
    std::fs::create_dir_all(dir.join(examples)).unwrap();
    for entry in std::fs::read_dir(Path::new(&fixtures()).join(examples)).unwrap() {
        let path = entry.unwrap().path();
        std::fs::copy(&path, dir.join(examples).join(path.file_name().unwrap())).unwrap();
    }
    angzarr_proto::protos::write_include_dir(&dir).unwrap();
    dir
}

/// `angzarr scaffold python` reading the protos from the directory `-I`
/// names (no `--proto-git`), run in `cwd`, with `extra`.
fn from_directory(cwd: &Path, include: &str, root: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_angzarr"))
        .args(["scaffold", "python", "--format", "json", "-I", include])
        .args(["--domain", "player", "--src"])
        .arg(root)
        .args(extra)
        .arg(PLAYER)
        .current_dir(cwd)
        .output()
        .unwrap()
}

/// A relative `-I` is relative to where the CLI runs, and buf runs in the
/// project: buf.gen.yaml names the directory, and the protos under it, from
/// the project root.
#[test]
fn scaffold_python_names_a_relative_directory_from_the_project_root() {
    for (include, src, named) in [
        ("spec/proto", "apps/player", "../../spec/proto"),
        ("./spec/x/../proto", "apps/./player", "../../spec/proto"),
        ("spec/proto", "spec/proto", "."),
        ("spec/proto", "spec/proto/player", ".."),
    ] {
        let cwd = empty_dir();
        std::fs::create_dir_all(cwd.join("spec/x")).unwrap();
        std::fs::rename(proto_dir(), cwd.join("spec/proto")).unwrap();
        let root = cwd.join(src);
        let out = from_directory(&cwd, include, &root, &["--no-proto-gen", "--no-lock"]);
        actions(&out, &root);
        let buf = std::fs::read_to_string(root.join("buf.gen.yaml")).unwrap();
        assert!(
            buf.contains(&format!(
                "  - directory: {named}\n    paths:\n      \
                 - {named}/io/angzarr/examples/v1/cards.proto\n"
            )),
            "-I {include} --src {src}:\n{buf}"
        );
        std::fs::remove_dir_all(&cwd).unwrap();
    }
}

/// An absolute `-I` is named as it is.
#[test]
fn scaffold_python_names_an_absolute_directory_as_given() {
    let protos = proto_dir();
    let root = empty_dir();
    let include = protos.display().to_string();
    let out = from_directory(&root, &include, &root, &["--no-proto-gen", "--no-lock"]);
    actions(&out, &root);
    let buf = std::fs::read_to_string(root.join("buf.gen.yaml")).unwrap();
    assert!(
        buf.contains(&format!(
            "  - directory: {include}\n    paths:\n      \
             - {include}/io/angzarr/examples/v1/cards.proto\n"
        )),
        "{buf}"
    );
    std::fs::remove_dir_all(&protos).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

/// buf generates the message types from a protos directory outside the
/// project, absolute or relative, its paths resolved under that directory.
#[test]
#[ignore = "needs the network (buf.build's remote plugins): run with --ignored"]
fn scaffold_python_generates_the_wire_types_from_a_directory_outside_the_project() {
    let protos = proto_dir();
    let base = empty_dir();
    std::fs::rename(proto_dir(), base.join("spec")).unwrap();
    for include in [protos.display().to_string(), "spec".to_string()] {
        let root = base.join("apps/player");
        let out = from_directory(&base, &include, &root, &["--no-lock"]);
        actions(&out, &root);
        let pb2 = std::fs::read_to_string(
            root.join("src/angzarr_player/_gen/io/angzarr/examples/v1/player_pb2.py"),
        )
        .unwrap_or_else(|e| panic!("-I {include}: {e}"));
        assert!(
            pb2.contains("from angzarr_player._gen.io.angzarr.examples.v1 import cards_pb2"),
            "{pb2}"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::remove_dir_all(&protos).unwrap();
    std::fs::remove_dir_all(&base).unwrap();
}
