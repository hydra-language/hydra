use std::{fs, path::Path};

use crate::{
    error::DriverError,
    style::{GREEN, RESET},
};

pub fn write_text(path: &Path, contents: &str, label: &str) -> Result<(), DriverError> {
    fs::write(path, contents)
        .map_err(|error| DriverError::io("failed to write", path, error))?;

    println!(
        "{}[INFO]{} {} written to: {}",
        GREEN,
        RESET,
        label,
        path.display(),
    );

    Ok(())
}

pub fn info(message: impl AsRef<str>) {
    println!(
        "{}[INFO]{} {}",
        GREEN,
        RESET,
        message.as_ref(),
    );
}
