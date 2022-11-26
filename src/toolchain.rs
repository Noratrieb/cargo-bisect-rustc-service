use std::process::Command;

use color_eyre::{
    eyre::{eyre, Context},
    Result,
};
use tracing::debug;

const MAX_BISECTOR_TOOLCHAINS: usize = 15;

#[tracing::instrument]
pub fn clean_toolchains() -> Result<()> {
    let toolchains = get_toolchains()?;
    let for_removal = filter_toolchain_for_removal(toolchains);
    if !for_removal.is_empty() {
        remove_toolchains(&for_removal)?;
    }

    Ok(())
}

fn filter_toolchain_for_removal(mut toolchains: Vec<String>) -> Vec<String> {
    toolchains.retain(|toolchain| toolchain.starts_with("bisector-"));

    let amount = toolchains.len();
    if amount <= MAX_BISECTOR_TOOLCHAINS {
        debug!(%amount, "No toolchains removed");
        return Vec::new();
    }

    let to_remove = amount - MAX_BISECTOR_TOOLCHAINS;

    toolchains.into_iter().take(to_remove).collect()
}

fn get_toolchains() -> Result<Vec<String>> {
    let mut command = Command::new("rustup");
    command.args(["toolchain", "list"]);

    let output = command
        .output()
        .wrap_err("running `rustup toolchain list`")?;

    if output.status.success() {
        let stdout =
            String::from_utf8(output.stdout).wrap_err("rustup returned non-utf-8 bytes")?;

        let toolchains = stdout.lines().map(ToOwned::to_owned).collect();

        Ok(toolchains)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        Err(eyre!("`rustup toolchain list` failed").wrap_err(stderr))
    }
}

fn remove_toolchains(toolchains: &[String]) -> Result<()> {
    debug!(?toolchains, "Removing toolchains");
    let mut command = Command::new("rustup");
    command.args(["toolchain", "remove"]);
    command.args(toolchains);

    let output = command
        .output()
        .wrap_err("running `rustup toolchain remove`")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Err(eyre!("`rustup toolchain remove` failed").wrap_err(stderr))
}
