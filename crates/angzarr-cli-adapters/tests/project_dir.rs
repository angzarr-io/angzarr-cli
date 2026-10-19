//! `.angzarr/` on disk, in fresh directories under the target's temporary
//! directory.

use std::path::PathBuf;

use angzarr_cli_adapters::DirProject;
use angzarr_cli_app::Project;
use angzarr_cli_core::project::{Config, Lock, NonTtyFormat, SpecCommit, Target};

const SHA: &str = "b1f00d2710eb7bee60308f6df520893d782012a6";

/// An empty directory of its own for each test.
fn fresh(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("project-dir")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn settings() -> (Config, Lock) {
    let mut config = Config::new([Target::Rust]);
    config.output.non_tty_format = Some(NonTtyFormat::Toml);
    (config, Lock::new(SpecCommit::parse("t", SHA).unwrap()))
}

#[test]
fn create_writes_config_lock_and_a_gitignore_for_the_cache() {
    let root = fresh("create");
    let project = DirProject::at(&root);
    assert!(!project.exists().unwrap());
    let (config, lock) = settings();
    project.create(&config, &lock).unwrap();
    assert!(project.exists().unwrap());
    assert_eq!(project.read_config().unwrap(), config);
    assert_eq!(project.read_lock().unwrap(), lock);
    let dir = root.join(".angzarr");
    assert_eq!(
        std::fs::read_to_string(dir.join(".gitignore")).unwrap(),
        "cache/\n"
    );
    assert!(!dir.join("cache").exists());
}

#[test]
fn create_refuses_an_existing_directory_and_leaves_it_alone() {
    let root = fresh("refuse");
    std::fs::create_dir(root.join(".angzarr")).unwrap();
    let project = DirProject::at(&root);
    let (config, lock) = settings();
    let dir = root.join(".angzarr").display().to_string();
    assert_eq!(project.dir(), dir);
    let err = project.create(&config, &lock).unwrap_err();
    assert_eq!(err.code(), "angzarr-cli-app-project-exists");
    assert_eq!(err.to_string(), format!("{dir} already exists"));
    assert!(err.remedy().is_some());
    assert_eq!(std::fs::read_dir(root.join(".angzarr")).unwrap().count(), 0);
}

#[test]
fn create_under_a_missing_parent_is_a_write_failure_not_exists() {
    let root = fresh("no-parent").join("absent");
    let (config, lock) = settings();
    let err = DirProject::at(&root).create(&config, &lock).unwrap_err();
    assert_eq!(err.code(), "angzarr-cli-adapters-project-write");
    assert!(!root.exists());
}

#[test]
fn a_missing_file_is_a_read_failure_naming_its_path() {
    let root = fresh("missing");
    std::fs::create_dir(root.join(".angzarr")).unwrap();
    let err = DirProject::at(&root).read_lock().unwrap_err();
    assert_eq!(err.code(), "angzarr-cli-adapters-project-read");
    let path = root.join(".angzarr").join("lock.yaml");
    assert!(
        err.to_string().starts_with(&path.display().to_string()),
        "{err}"
    );
}

#[test]
fn discover_finds_the_nearest_config_walking_up() {
    let root = fresh("discover");
    let (config, lock) = settings();
    DirProject::at(&root).create(&config, &lock).unwrap();
    let deep = root.join("a").join("b");
    std::fs::create_dir_all(&deep).unwrap();
    assert_eq!(DirProject::discover(&deep), Some(DirProject::at(&root)));
    assert_eq!(DirProject::discover(&root), Some(DirProject::at(&root)));

    // A nearer project wins; a directory without config.yaml is not one.
    let inner = root.join("a");
    DirProject::at(&inner).create(&config, &lock).unwrap();
    assert_eq!(DirProject::discover(&deep), Some(DirProject::at(&inner)));
    std::fs::remove_file(inner.join(".angzarr").join("config.yaml")).unwrap();
    assert_eq!(DirProject::discover(&deep), Some(DirProject::at(&root)));
}
