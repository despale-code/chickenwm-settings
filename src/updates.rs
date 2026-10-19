//! Settings > General > Updates: runs the installer's update mode (docs/update.ps1 on the
//! website). The check runs hidden and reads the installer's "@@ <repo> <state> [text]" lines;
//! the update itself opens a PowerShell window, since it closes every ChickenWM program
//! (this one included) to replace them.

use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

const UPDATE_SCRIPT: &str = "https://despale-code.github.io/chickenwm/update.ps1";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

#[derive(Clone, Default)]
pub(crate) enum CheckState {
    #[default]
    Idle,
    Checking,
    /// Programs with an update, as (repository, new commit).
    Done(Vec<(String, String)>),
    Failed(String),
}

#[derive(Default)]
pub(crate) struct Updates {
    state: Arc<Mutex<CheckState>>,
}

fn powershell(mode: &str) -> Command {
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass"])
        .env("CHICKENWM_UPDATE", mode);
    cmd
}

impl Updates {
    pub fn state(&self) -> CheckState {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Starts a background check; `ctx` is repainted when it finishes.
    pub fn check(&self, ctx: &eframe::egui::Context) {
        let state = Arc::clone(&self.state);
        if let Ok(mut s) = state.lock() {
            if matches!(*s, CheckState::Checking) {
                return;
            }
            *s = CheckState::Checking;
        }
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = run_check();
            if let Ok(mut s) = state.lock() {
                *s = match result {
                    Ok(list) => CheckState::Done(list),
                    Err(e) => CheckState::Failed(e),
                };
            }
            ctx.request_repaint();
        });
    }

    /// Opens the updater in its own window.
    pub fn update_now() -> Result<(), String> {
        powershell("1")
            .args(["-NoExit", "-Command", &format!("irm '{UPDATE_SCRIPT}' | iex")])
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Couldn't start PowerShell: {e}"))
    }
}

fn run_check() -> Result<Vec<(String, String)>, String> {
    let mut child = powershell("check")
        .env("CHICKENWM_MACHINE", "1")
        .args(["-Command", &format!("irm '{UPDATE_SCRIPT}' | iex")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Couldn't start PowerShell: {e}"))?;
    let mut available = Vec::new();
    let mut outcome = None;
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            match parse_report(&line) {
                Some(Report::Available(repo, sha)) => available.push((repo, sha)),
                Some(Report::Done) => outcome = Some(Ok(())),
                Some(Report::Error(text)) => outcome = Some(Err(text)),
                None => {}
            }
        }
    }
    let _ = child.wait();
    match outcome {
        Some(Ok(())) => Ok(available),
        Some(Err(e)) => Err(e),
        None => Err("The update check didn't finish (no internet connection?)".to_string()),
    }
}

#[derive(Debug, PartialEq)]
enum Report {
    Available(String, String),
    Done,
    Error(String),
}

/// Parses one "@@ <repo> <state> [text]" progress line; other lines and states are ignored.
fn parse_report(line: &str) -> Option<Report> {
    let rest = line.trim().strip_prefix("@@ ")?;
    let (head, tail) = rest.split_once(' ').unwrap_or((rest, ""));
    match head {
        "done" => Some(Report::Done),
        "error" => Some(Report::Error(tail.trim().to_string())),
        repo => {
            let (state, text) = tail.split_once(' ').unwrap_or((tail, ""));
            (state == "available").then(|| Report::Available(repo.to_string(), text.trim().to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_installer_reports() {
        assert_eq!(
            parse_report("@@ roobar available 1a2b3c4"),
            Some(Report::Available("roobar".into(), "1a2b3c4".into()))
        );
        assert_eq!(parse_report("@@ roobar current"), None);
        assert_eq!(parse_report("@@ done 2"), Some(Report::Done));
        assert_eq!(parse_report("@@ error No internet"), Some(Report::Error("No internet".into())));
        assert_eq!(parse_report("ChickenWM updater"), None);
    }
}
