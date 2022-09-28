use std::{fs, process::Command, sync::mpsc};

use color_eyre::eyre::Context;
use rusqlite::Connection;
use serde::Serialize;
use tracing::{error, info};
use uuid::Uuid;

use crate::Options;

#[derive(Debug, Serialize)]
pub enum BisectStatus {
    InProgress,
    Error(String),
    Success(String),
}

#[derive(Debug, Serialize)]
pub struct Bisection {
    pub id: Uuid,
    pub code: String,
    pub status: BisectStatus,
}

pub struct Job {
    id: Uuid,
    code: String,
    options: Options,
}

enum JobState {
    Failed(String),
    Success(String),
}

impl JobState {
    fn status(&self) -> &'static str {
        match self {
            Self::Failed(_) => "error",
            Self::Success(_) => "success",
        }
    }
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job")
            .field("id", &self.id)
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl Job {
    pub fn new(id: Uuid, code: String, options: Options) -> Self {
        Self { id, code, options }
    }
}

pub fn bisect_worker(jobs: mpsc::Receiver<Job>, conn: Connection) {
    loop {
        let job = match jobs.recv() {
            Ok(job) => job,
            Err(_) => return,
        };

        info!(id = %job.id, "Starting bisection job");

        bisect_job(job);
    }
}

#[tracing::instrument(skip(job), fields(id = %job.id))]
fn bisect_job(job: Job) {
    match run_bisect_for_file(job.code, job.options.start, job.options.end) {
        Ok(state) => {
            info!(state = %state.status(), "Bisection finished");
        }
        Err(err) => {
            error!(?err, "Error during bisection");
        }
    }
}

fn run_bisect_for_file(
    input: String,
    start: Option<chrono::NaiveDate>,
    end: Option<chrono::NaiveDate>,
) -> color_eyre::Result<JobState> {
    let temp_dir = tempdir::TempDir::new("bisect").wrap_err("creating tempdir")?;
    let mut cargo_new = Command::new("cargo");
    cargo_new
        .arg("new")
        .arg("bisect")
        .arg("--lib") // lib even works with `fn main() {}`
        .current_dir(&temp_dir);

    let output = cargo_new.output().wrap_err("cargo init")?;
    output
        .status
        .exit_ok()
        .wrap_err_with(|| format!("running cargo: {}", String::from_utf8_lossy(&output.stderr)))?;

    let cargo_dir = temp_dir.path().join("bisect");

    fs::write(cargo_dir.join("src").join("lib.rs"), input).wrap_err("writing code to lib.rs")?;

    let mut bisect = Command::new("cargo-bisect-rustc");
    bisect.arg("--preserve"); // preserve toolchains for future runs
    bisect.arg("--access").arg("github"); // ask the github api about the commits
    bisect.arg("--timeout").arg("30"); // don't hang
    bisect.current_dir(&cargo_dir);

    if let Some(start) = start {
        bisect.arg("--start").arg(start.to_string());
    }

    if let Some(end) = end {
        bisect.arg("--end").arg(end.to_string());
    }

    let output = bisect.output().wrap_err("spawning cargo-bisect-rustc")?;

    if output.status.success() {
        Ok(JobState::Success(
            String::from_utf8(output.stdout)
                .wrap_err("cargo-bisect-rustc stdout utf8 validation")?,
        ))
    } else {
        Ok(JobState::Failed(
            String::from_utf8(output.stderr)
                .wrap_err("cargo-bisect-rustc stderr utf8 validation")?,
        ))
    }
}
