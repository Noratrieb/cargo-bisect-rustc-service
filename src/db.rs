use color_eyre::eyre::Context;
use rusqlite::Connection;
use tracing::info;

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

pub fn add_bisection(conn: &Connection) -> color_eyre::Result<()> {
    Ok(())
}

pub fn get_bisections(conn: &Connection) -> color_eyre::Result<Vec<Bisection>> {
    let mut select = conn
        .prepare("SELECT job_id, code, status, stdout, stderr FROM bisect")
        .wrap_err("preparing select")?;

    let iter = select
        .query_map([], |row| {
            Ok(Bisection {
                id: row.get(0)?,
                code: row.get(1)?,
                status: match row.get(2)? {
                    0 => BisectStatus::InProgress,
                    1 => BisectStatus::Error(row.get(3)?),
                    2 => BisectStatus::Success(row.get(3)?),
                    _ => return Err(rusqlite::Error::InvalidQuery), // actually not lol
                },
            })
        })
        .wrap_err("getting bisections from db query")?;

    iter.collect::<Result<Vec<_>, rusqlite::Error>>()
        .wrap_err("getting bisections from db")
}
