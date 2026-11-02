//! [`TestDir`]: a test's directory under the temporary directory, removed
//! when the guard drops, on a pass and on a panic alike, so a failing (or
//! mutated) test leaves nothing behind.
//!
//! Every crate's tests include this one file (`#[path]`), so the workspace
//! has one rule for where a test's directory is and when it goes:
//!
//! - It is `<temp_dir>/<prefix>-<pid>-<n>`, `n` counting within the test
//!   binary: short, for the tests that put a socket in it, and fresh (a
//!   leftover of that name is removed first).
//! - It is removed when the guard drops. Bind the guard for as long as the
//!   directory is used: `let dir = TestDir::new(..)`, never
//!   `TestDir::new(..).join(..)`, whose guard drops at once.
//! - To keep a failing test's directory for debugging, set
//!   `ANGZARR_KEEP_TEST_DIRS` (to anything): a guard dropped while its test
//!   panics then keeps the directory and names it on stderr. A passing
//!   test's directory goes either way.

#![allow(dead_code)]

use std::ffi::OsStr;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The environment variable that keeps a panicking test's directory.
pub const KEEP: &str = "ANGZARR_KEEP_TEST_DIRS";

/// A directory removed when this drops, unless it drops in a panic with
/// [`KEEP`] set.
#[derive(Debug)]
pub struct TestDir {
    path: PathBuf,
    keep_on_panic: bool,
}

impl TestDir {
    /// A fresh, empty `<temp_dir>/<prefix>-<pid>-<n>`, kept on a panic when
    /// [`KEEP`] is set.
    pub fn new(prefix: &str) -> TestDir {
        TestDir::keeping_on_panic(prefix, std::env::var_os(KEEP).is_some())
    }

    /// [`TestDir::new`], kept on a panic when `keep_on_panic`, whatever the
    /// environment holds.
    pub fn keeping_on_panic(prefix: &str, keep_on_panic: bool) -> TestDir {
        static RUN: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            RUN.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)
            .unwrap_or_else(|e| panic!("creating {}: {e}", path.display()));
        TestDir {
            path,
            keep_on_panic,
        }
    }

    /// The directory.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        if self.keep_on_panic && std::thread::panicking() {
            eprintln!("{KEEP}: kept {}", self.path.display());
            return;
        }
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

impl Deref for TestDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<OsStr> for TestDir {
    fn as_ref(&self) -> &OsStr {
        self.path.as_os_str()
    }
}
