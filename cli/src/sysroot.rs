use std::{env, path::{Path, PathBuf}};

#[derive(Debug, Clone)]
pub struct Sysroot {
    root: PathBuf,
}

impl Sysroot {

    pub fn discover(explicit: Option<PathBuf>) -> Result<Self, String> {
        // 1. explicit command-line override.
        if let Some(root) = explicit {
            return Self::from_root(root);
        }

        // 2. environment override.
        if let Some(root) = env::var_os("HYDRA_SYSROOT") {
            return Self::from_root(PathBuf::from(root));
        }

        // 3. installed layout:
        //
        //     <sysroot>/bin/hydrac
        //     <sysroot>/stdlib
        //     <sysroot>/runtime
        //
        if let Ok(exe) = env::current_exe() {
            if let Some(root) = exe.parent().and_then(Path::parent) {
                if root.join("stdlib").is_dir() {
                    return Self::from_root(root.to_path_buf());
                }
            }
        }

        // 4. development fallback.
        //
        // CARGO_MANIFEST_DIR is expected to be:
        //
        //     hydra/crates/<compiler-crate>
        //
        // so ../.. is the Hydra repository root.
        let dev_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..");

        if dev_root.join("stdlib").is_dir() {
            return Self::from_root(dev_root);
        }

        Err(
            "could not locate the Hydra sysroot; \
             pass `--sysroot <path>` or set `HYDRA_SYSROOT`"
                .to_string(),
        )
    }

    fn from_root(root: PathBuf) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|e| {
            format!(
                "could not resolve Hydra sysroot `{}`: {}",
                root.display(),
                e,
            )
        })?;

        let stdlib = root.join("stdlib");

        if !stdlib.is_dir() {
            return Err(format!(
                "Hydra sysroot `{}` does not contain `stdlib/`",
                root.display(),
            ));
        }

        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn stdlib(&self) -> PathBuf {
        self.root.join("stdlib")
    }

    pub fn runtime(&self) -> PathBuf {
        self.root.join("runtime")
    }

    pub fn runtime_arch(&self) -> PathBuf {
        self.runtime().join("arch")
    }
}
