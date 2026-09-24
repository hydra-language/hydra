use std::{
    env,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    error::DriverError,
    session::Session,
};

pub fn link(
    session: &Session,
    object: &Path,
    executable: &Path,
) -> Result<(), DriverError> {
    if cfg!(target_os = "windows") {
        let status = Command::new("clang")
            .arg(object)
            .arg("-o")
            .arg(executable)
            .status()
            .map_err(|error| {
                DriverError::message(
                    format!("failed to invoke linker: {}", error)
                )
            })?;

        if !status.success() {
            return Err(DriverError::message("linking failed"));
        }

        return Ok(());
    }

    let start_object = assemble_start(session)?;
    let runtime_lib = runtime_library(session)?;

    let status = Command::new("clang")
        .arg("-nostartfiles")
        .arg("-o")
        .arg(executable)
        .arg(&start_object)
        .arg(object)
        .arg(&runtime_lib)
        .status()
        .map_err(|error| {
            DriverError::message(
                format!("failed to invoke linker: {}", error)
            )
        })?;

    if !status.success() {
        return Err(DriverError::message("linking failed"));
    }

    Ok(())
}

fn assemble_start(session: &Session) -> Result<PathBuf, DriverError> {
    let runtime_dir = session.sysroot.runtime_arch();

    let start_s = match env::consts::ARCH {
        "x86_64" => runtime_dir.join("x86_64/start.s"),
        "aarch64" => runtime_dir.join("aarch64/start.s"),

        architecture => {
            return Err(DriverError::message(
                format!(
                    "unsupported runtime architecture `{}`",
                    architecture,
                )
            ));
        }
    };

    if !start_s.exists() {
        return Err(DriverError::message(
            format!(
                "runtime start file not found: {}",
                start_s.display(),
            )
        ));
    }

    let start_o = start_s.with_extension("o");

    let needs_rebuild = !start_o.exists() || modified(&start_o)? < modified(&start_s)?;

    if needs_rebuild {
        let status = Command::new("as")
            .arg(&start_s)
            .arg("-o")
            .arg(&start_o)
            .status()
            .map_err(|error| {
                DriverError::message(
                    format!("failed to assemble start.s: {}", error)
                )
            })?;

        if !status.success() {
            return Err(
                DriverError::message(
                    "failed to assemble runtime start file"
                )
            );
        }
    }

    Ok(start_o)
}

fn runtime_library(session: &Session) -> Result<PathBuf, DriverError> {
    let runtime_lib = session
        .sysroot
        .root()
        .join("target")
        .join(session.options.runtime_profile())
        .join("librt.a");

    if !runtime_lib.exists() {
        return Err(DriverError::message(
            format!(
                "runtime library not found: {}",
                runtime_lib.display(),
            )
        ));
    }

    Ok(runtime_lib)
}

fn modified(path: &Path) -> Result<std::time::SystemTime, DriverError> {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| {
            DriverError::io(
                "failed to read metadata for",
                path,
                error,
            )
        })
}
