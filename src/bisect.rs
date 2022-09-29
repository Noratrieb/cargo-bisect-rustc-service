use std::process::Output;
use std::{fs, process::Command, sync::mpsc};

use color_eyre::eyre::{Context, ContextCompat};
use color_eyre::Result;
use rusqlite::Connection;
use serde::Serialize;
use tracing::{error, info, trace};
use uuid::Uuid;

use crate::{db, Options};

#[derive(Debug, Serialize)]
#[serde(tag = "status")]
pub enum BisectStatus {
    InProgress,
    Error { output: String },
    Success { output: String },
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
    Failed,
    Success,
}

impl JobState {
    fn status(&self) -> &'static str {
        match self {
            Self::Failed => "error",
            Self::Success => "success",
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

        match process_job(job, &conn) {
            Ok(()) => {}
            Err(err) => {
                error!(?err, "error processing bisection")
            }
        }
    }
}

#[tracing::instrument(skip(job, conn), fields(id = %job.id))]
pub fn process_job(job: Job, conn: &Connection) -> Result<()> {
    info!(id = %job.id, "Starting bisection job");

    let mut bisect = Bisection {
        id: job.id,
        code: job.code.clone(),
        status: BisectStatus::InProgress,
    };

    db::add_bisection(&conn, &bisect).wrap_err("insert bisection")?;

    let status = match bisect_job(job) {
        Ok(status) => status,
        Err(err) => {
            error!(?err, "error processing bisection");
            BisectStatus::Error {
                output: format!("Internal error"),
            }
        }
    };

    bisect.status = status;

    db::update_bisection_status(&conn, &bisect).wrap_err("writing bisection result")?;

    trace!(?bisect, "Finished bisection job");

    Ok(())
}

fn bisect_job(job: Job) -> Result<BisectStatus> {
    let (output, state) = run_bisect_for_file(job.code, &job.options)?;
    info!(state = %state.status(), "Bisection finished");

    process_result(output, state).wrap_err("process result")
}

fn process_result(output: Output, state: JobState) -> Result<BisectStatus> {
    let stderr =
        String::from_utf8(output.stderr).wrap_err("cargo-bisect-rustc stderr utf8 validation")?;

    match state {
        JobState::Failed => {
            let mut output = stderr.lines().rev().take(30).collect::<Vec<_>>();
            output.reverse();
            let output = output.join("\n");
            info!(?output, "output");
            Ok(BisectStatus::Error { output })
        }
        JobState::Success => {
            let cutoff = stderr.rfind("searched nightlies:").wrap_err_with(|| {
                format!("cannot find `searched nightlies:` in output. output:\n{stderr}")
            })?;
            let output = stderr[cutoff..].to_string();
            Ok(BisectStatus::Success { output })
        }
    }
}

fn run_bisect_for_file(input: String, options: &Options) -> Result<(Output, JobState)> {
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

    bisect.arg("--start").arg(options.start.to_string());

    if let Some(end) = options.end {
        bisect.arg("--end").arg(end.to_string());
    }

    bisect
        .arg("--regress")
        .arg(options.kind.as_deref().unwrap_or("ice")); // FIXME Make this configurable

    bisect.env("RUST_LOG", "error"); // overwrite RUST_LOG

    let output = bisect.output().wrap_err("spawning cargo-bisect-rustc")?;

    if output.status.success() {
        Ok((output, JobState::Success))
    } else {
        Ok((output, JobState::Failed))
    }
}
