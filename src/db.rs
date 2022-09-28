use color_eyre::eyre::Context;
use rusqlite::Connection;
use tracing::info;
use uuid::Uuid;

use crate::bisect::{BisectStatus, Bisection};

pub fn setup(conn: &Connection) -> color_eyre::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS bisect (
            job_id STRING PRIMARY KEY,
            code STRING NOT NULL,
            status INTEGER NOT NULL DEFAULT 0,
            stdout_stderr STRING -- stdout or stderr depending on the status
        )",
        (),
    )
    .wrap_err("setup sqlite table")?;

    info!("Finished db setup");

    Ok(())
}

fn status_to_sql(status: &BisectStatus) -> (u8, Option<&str>) {
    match status {
        BisectStatus::InProgress => (0, None),
        BisectStatus::Error { output } => (1, Some(&output)),
        BisectStatus::Success { output } => (2, Some(&output)),
    }
}

pub fn add_bisection(conn: &Connection, bisect: &Bisection) -> color_eyre::Result<()> {
    let (status, stdout_stderr) = status_to_sql(&bisect.status);

    conn.execute(
        "INSERT INTO bisect (job_id, code, status, stdout_stderr) VALUES (?1, ?2, ?3, ?4)",
        (bisect.id, &bisect.code, status, stdout_stderr),
    )
    .wrap_err("insert into database")
    .map(drop)
}

pub fn update_bisection_status(conn: &Connection, bisect: &Bisection) -> color_eyre::Result<()> {
    let (status, stdout_stderr) = status_to_sql(&bisect.status);

    conn.execute(
        "UPDATE bisect SET status = ?1, stdout_stderr = ?2 WHERE bisect.job_id = ?3",
        (status, stdout_stderr, bisect.id),
    )
    .wrap_err("insert into database")
    .map(drop)
}

pub fn get_bisections(conn: &Connection) -> color_eyre::Result<Vec<Bisection>> {
    let mut select = conn
        .prepare("SELECT job_id, code, status, stdout_stderr FROM bisect")
        .wrap_err("preparing select")?;

    let iter = select
        .query_map([], |row| {
            Ok(Bisection {
                id: row.get(0)?,
                code: row.get(1)?,
                status: match row.get(2)? {
                    0 => BisectStatus::InProgress,
                    1 => BisectStatus::Error {
                        output: row.get(3)?,
                    },
                    2 => BisectStatus::Success {
                        output: row.get(3)?,
                    },
                    _ => return Err(rusqlite::Error::InvalidQuery), // actually not lol
                },
            })
        })
        .wrap_err("getting bisections from db query")?;

    iter.collect::<Result<Vec<_>, rusqlite::Error>>()
        .wrap_err("getting bisections from db")
}

pub fn get_bisection(conn: &Connection, id: Uuid) -> color_eyre::Result<Option<Bisection>> {
    let mut select = conn
        .prepare("SELECT job_id, code, status, stdout_stderr FROM bisect WHERE job_id = ?1")
        .wrap_err("preparing select")?;

    let mut iter = select
        .query_map([id], |row| {
            Ok(Bisection {
                id: row.get(0)?,
                code: row.get(1)?,
                status: match row.get(2)? {
                    0 => BisectStatus::InProgress,
                    1 => BisectStatus::Error {
                        output: row.get(3)?,
                    },
                    2 => BisectStatus::Success {
                        output: row.get(3)?,
                    },
                    _ => return Err(rusqlite::Error::InvalidQuery), // actually not lol
                },
            })
        })
        .wrap_err("getting bisections from db query")?;

    iter.next().transpose().wrap_err("getting bisection")
}
