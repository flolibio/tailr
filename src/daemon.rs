//! Daemon mode support for tailr.
//!
//! Provides daemonization, PID file management, graceful shutdown,
//! and service file generation for systemd (Linux) and launchd (macOS).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use daemonize::Daemonize;

/// Returns the tailr data directory (`~/.local/share/tailr` or XDG equivalent).
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".local").join("share")
        })
        .join("tailr")
}

/// Returns the path to the PID file.
pub fn pid_file() -> PathBuf {
    data_dir().join("tailr.pid")
}

/// Returns the path to the daemon log file.
pub fn log_file() -> PathBuf {
    data_dir().join("tailr.log")
}

/// Configuration for daemon mode.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// Path to the PID file.
    pub pid_file: PathBuf,
    /// Path to the log file (stdout/stderr redirected here).
    pub log_file: PathBuf,
    /// Working directory for the daemon process.
    pub working_dir: PathBuf,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            pid_file: pid_file(),
            log_file: log_file(),
            working_dir: data_dir(),
        }
    }
}

/// Daemonize the current process.
///
/// Forks into the background, redirects stdout/stderr to the log file,
/// writes the PID file, and sets umask to `0o027`. The parent process exits
/// cleanly on success. Prints an error and calls `process::exit(1)` on failure.
pub fn daemonize_process(config: &DaemonConfig) {
    if let Some(parent) = config.pid_file.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("Failed to create data directory {}: {}", parent.display(), e);
            std::process::exit(1);
        }
    }

    let stdout = match fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.log_file)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "Failed to open log file {}: {}",
                config.log_file.display(),
                e
            );
            std::process::exit(1);
        }
    };
    let stderr = match fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.log_file)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "Failed to open log file {}: {}",
                config.log_file.display(),
                e
            );
            std::process::exit(1);
        }
    };

    let daemon = Daemonize::new()
        .pid_file(&config.pid_file)
        .working_directory(&config.working_dir)
        .umask(0o027)
        .stdout(stdout)
        .stderr(stderr);

    if let Err(e) = daemon.start() {
        eprintln!("Failed to daemonize: {}", e);
        std::process::exit(1);
    }
}

/// Stop a running tailr daemon.
///
/// Reads the PID from the PID file, sends SIGTERM, waits up to `timeout_secs`
/// for the process to exit, then removes the PID file.
pub fn stop_daemon(pid_path: Option<&PathBuf>, timeout_secs: u64) -> Result<(), String> {
    let pid_path = pid_path.cloned().unwrap_or_else(pid_file);
    let pid = read_pid(&pid_path)?;

    let status = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .map_err(|e| format!("Failed to send SIGTERM to PID {}: {}", pid, e))?;

    if !status.success() {
        return Err(format!(
            "kill -TERM {} exited with status {}",
            pid, status
        ));
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        if !is_process_running(pid) {
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "Process {} did not exit within {} seconds",
                pid, timeout_secs
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    cleanup_pid_file(Some(&pid_path));
    Ok(())
}

/// Check the status of the tailr daemon.
///
/// Returns a human-readable message: running (with PID), stopped, or no PID file.
pub fn daemon_status(pid_path: Option<&PathBuf>) -> String {
    let pid_path = pid_path.cloned().unwrap_or_else(pid_file);

    let pid = match read_pid(&pid_path) {
        Ok(p) => p,
        Err(_) => return "tailr is not running (no PID file found)".to_string(),
    };

    if is_process_running(pid) {
        format!("tailr is running (PID {})", pid)
    } else {
        format!(
            "tailr is not running (stale PID file for PID {})",
            pid
        )
    }
}

/// Restart a running tailr daemon.
///
/// Stops the current daemon (via `stop_daemon`), then re-launches it according to
/// the detected runtime environment:
/// - **systemd / launchd**: relies on the unit/plist `Restart=` policy to bring
///   the process back; this function only waits for a new PID to appear.
/// - **manual / daemonize mode**: re-execs the current binary with the original
///   CLI args (`std::env::args`), preserving `-l`/`-b`/`--daemon` etc.
///
/// Synchronous by design: `restart` is a one-shot CLI command with no concurrent
/// work to yield to, matching the `stop_daemon` polling style (`std::thread::sleep`).
pub fn restart_daemon(pid_path: Option<&PathBuf>) -> Result<(), String> {
    let resolved = pid_path.cloned().unwrap_or_else(pid_file);
    let old_pid = read_pid(&resolved).ok();

    // 1. Stop the running daemon (5s timeout, cleans PID file on success).
    stop_daemon(pid_path, 5)?;

    // 2. Re-launch per runtime environment.
    if is_systemd_service() || is_launchd_service() {
        // systemd (Type=forking + Restart=on-failure) / launchd (KeepAlive) auto-restart.
        wait_for_new_pid(&resolved, old_pid, 10)?;
    } else {
        // Manual / daemonize mode: re-exec self with original args.
        let exe = std::env::current_exe()
            .map_err(|e| format!("failed to resolve current exe: {}", e))?;
        let args: Vec<String> = std::env::args().skip(1).collect();
        std::process::Command::new(exe)
            .args(&args)
            .spawn()
            .map_err(|e| format!("failed to re-exec: {}", e))?;
        wait_for_new_pid(&resolved, old_pid, 10)?;
    }
    Ok(())
}

/// Detect whether this process was launched by systemd.
///
/// systemd sets `INVOCATION_ID` per service invocation — a reliable signal that
/// *this* process is managed by systemd (unlike `/run/systemd/system` which only
/// indicates systemd is installed on the host).
pub fn is_systemd_service() -> bool {
    std::env::var_os("INVOCATION_ID").is_some()
}

/// Detect whether this process was launched by launchd (macOS).
///
/// launchd sets `LaunchInstanceID` for managed services.
pub fn is_launchd_service() -> bool {
    std::env::var_os("LaunchInstanceID").is_some()
}

/// Poll the PID file until a new (different from `old_pid`) running PID appears.
fn wait_for_new_pid(
    pid_path: &PathBuf,
    old_pid: Option<u32>,
    timeout_secs: u64,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        if let Ok(pid) = read_pid(pid_path) {
            if Some(pid) != old_pid && is_process_running(pid) {
                return Ok(());
            }
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "daemon did not come back within {} seconds",
                timeout_secs
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
}

/// Generate a systemd service unit file for tailr.
///
/// Uses `Type=forking` with `PIDFile` to work with the daemonize-based
/// process management.
pub fn generate_systemd_service(
    binary_path: &str,
    log_dirs: &[String],
    user: &str,
    group: &str,
) -> String {
    let log_args = log_dirs.join(" ");

    format!(
        r#"[Unit]
Description=tailr - Log tail and search server
After=network.target

[Service]
Type=forking
PIDFile={data_dir}/tailr.pid
ExecStart={binary} --log {log_args}
ExecStop=/bin/kill -TERM $MAINPID
Restart=on-failure
RestartSec=5
User={user}
Group={group}
WorkingDirectory={data_dir}

[Install]
WantedBy=multi-user.target
"#,
        data_dir = data_dir().display(),
        binary = binary_path,
        log_args = log_args,
        user = user,
        group = group,
    )
}

/// Generate a macOS launchd plist file for tailr.
pub fn generate_launchd_plist(binary_path: &str, log_dirs: &[String]) -> String {
    let log_args: Vec<String> = log_dirs
        .iter()
        .map(|d| format!("        <string>--log</string>\n        <string>{}</string>", d))
        .collect();
    let program_args = log_args.join("\n");

    let label = "com.tailr.server";
    let log_path = log_file().display().to_string();

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>

    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
{program_args}
    </array>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <true/>

    <key>StandardOutPath</key>
    <string>{log}</string>

    <key>StandardErrorPath</key>
    <string>{log}</string>

    <key>ProcessType</key>
    <string>Background</string>
</dict>
</plist>
"#,
        label = label,
        binary = binary_path,
        program_args = program_args,
        log = log_path,
    )
}

/// Remove the PID file if it exists. Call on graceful shutdown.
pub fn cleanup_pid_file(path: Option<&PathBuf>) {
    let path = path.cloned().unwrap_or_else(pid_file);
    if path.exists() {
        if let Err(e) = fs::remove_file(&path) {
            tracing::warn!(path = %path.display(), error = %e, "failed to remove PID file");
        }
    }
}

/// Wait for a Unix shutdown signal (SIGINT, SIGTERM, or SIGHUP).
///
/// Use inside `tokio::select!` to trigger graceful shutdown.
pub async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
    let mut sigterm = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
    let mut sighup = signal(SignalKind::hangup()).expect("failed to install SIGHUP handler");

    tokio::select! {
        _ = sigint.recv() => {
            tracing::info!("received SIGINT, shutting down");
        }
        _ = sigterm.recv() => {
            tracing::info!("received SIGTERM, shutting down");
        }
        _ = sighup.recv() => {
            tracing::info!("received SIGHUP, shutting down");
        }
    }
}

fn read_pid(path: &PathBuf) -> Result<u32, String> {
    let contents = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read PID file {}: {}", path.display(), e))?;

    contents
        .trim()
        .parse::<u32>()
        .map_err(|e| format!("Invalid PID in {}: {}", path.display(), e))
}

/// Uses `kill -0` to check process existence without sending a signal.
fn is_process_running(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
