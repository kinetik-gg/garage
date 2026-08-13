//! The three file writers. Which one a path gets is a load-bearing decision:
//! see each submodule's docs, ported from the Python they replace.

pub mod atomic;
pub mod lua;
pub mod marker;

#[cfg(test)]
pub(crate) mod scratch {
    //! A directory of its own per test, so two tests never race over one name.

    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SERIAL: AtomicU64 = AtomicU64::new(0);

    /// A temporary directory that removes itself, and everything under it, when the test
    /// that made it ends.
    #[derive(Debug)]
    pub(crate) struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        pub(crate) fn new(label: &str) -> Self {
            let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "garage-core-{label}-{}-{serial}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("scratch directory is creatable");
            Self { path }
        }

        pub(crate) fn path(&self) -> &Path {
            &self.path
        }

        pub(crate) fn join(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.path));
        }
    }
}
