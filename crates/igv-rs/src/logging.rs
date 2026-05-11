use std::path::PathBuf;
use std::str::FromStr;

use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

pub fn setup(level: &str) -> anyhow::Result<WorkerGuard> {
    let log_dir = state_dir()?;
    std::fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::never(&log_dir, "debug.log");
    let (writer, guard) = tracing_appender::non_blocking(file_appender);

    let lvl = Level::from_str(&level.to_uppercase()).unwrap_or(Level::INFO);
    let filter = EnvFilter::new(format!("igv_rs={lvl},igv_core={lvl}"));

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    Ok(guard)
}

fn state_dir() -> anyhow::Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "igv-rs")
        .ok_or_else(|| anyhow::anyhow!("no project dir"))?;
    Ok(dirs.data_local_dir().to_path_buf())
}
