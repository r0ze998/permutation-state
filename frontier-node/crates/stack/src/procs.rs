//! Component processes: spawn in their own process group (so they outlive
//! `up` and a Ctrl-C of the terminal does not reach them), log to
//! `logs/<name>.log`, signal by pid (`kill`), reap.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// How to (re)start one component.
#[derive(Clone, Debug)]
pub struct Spec {
    pub name: String,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    pub log: PathBuf,
    /// A component that finishes by itself (the bots) is not restarted
    /// after a clean exit.
    pub finishes: bool,
}

impl Spec {
    pub fn to_json(&self) -> Value {
        json!({"name": self.name, "program": self.program.display().to_string(), "args": self.args,
               "cwd": self.cwd.display().to_string(), "log": self.log.display().to_string(), "finishes": self.finishes})
    }
}

/// One running (or stopped) component.
pub struct Proc {
    pub spec: Spec,
    pub child: Option<Child>,
    pub pid: Option<u32>,
    pub started: Option<Instant>,
    pub starts: u32,
    /// Killed on purpose (chaos): restart at this instant.
    pub restart_at: Option<Instant>,
    /// Exited cleanly and does not restart.
    pub done: bool,
    pub last_exit: Option<String>,
}

impl Proc {
    pub fn new(spec: Spec) -> Proc {
        Proc {
            spec,
            child: None,
            pid: None,
            started: None,
            starts: 0,
            restart_at: None,
            done: false,
            last_exit: None,
        }
    }

    pub fn start(&mut self) -> Result<u32, String> {
        let child = spawn(&self.spec)?;
        let pid = child.id();
        self.child = Some(child);
        self.pid = Some(pid);
        self.started = Some(Instant::now());
        self.starts += 1;
        self.restart_at = None;
        self.done = false;
        Ok(pid)
    }

    /// Reaps an exit: `Some(description)` once when the process ended.
    pub fn poll_exit(&mut self) -> Option<String> {
        let c = self.child.as_mut()?;
        match c.try_wait() {
            Ok(Some(st)) => {
                self.child = None;
                self.pid = None;
                let d = match st.code() {
                    Some(c) => format!("exit {c}"),
                    None => format!("signal ({st})"),
                };
                if self.spec.finishes && st.success() {
                    self.done = true;
                }
                self.last_exit = Some(d.clone());
                Some(d)
            }
            _ => None,
        }
    }

    pub fn running(&self) -> bool {
        self.child.is_some()
    }

    /// `kill -9` and reap (chaos).
    pub fn kill9(&mut self) -> Result<(), String> {
        let Some(pid) = self.pid else {
            return Err(format!("{} is not running", self.spec.name));
        };
        signal(pid, "KILL")?;
        if let Some(mut c) = self.child.take() {
            let _ = c.wait();
        }
        self.pid = None;
        self.last_exit = Some("kill -9 (chaos)".into());
        Ok(())
    }

    /// SIGINT (the binaries checkpoint on Ctrl-C), then SIGKILL after
    /// `grace`.
    pub fn stop(&mut self, grace: Duration) {
        if let Some(pid) = self.pid {
            let _ = signal(pid, "INT");
            let t0 = Instant::now();
            loop {
                if self.poll_exit().is_some() || !alive(pid) {
                    break;
                }
                if t0.elapsed() > grace {
                    let _ = signal(pid, "KILL");
                    if let Some(mut c) = self.child.take() {
                        let _ = c.wait();
                    }
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        self.child = None;
        self.pid = None;
        self.restart_at = None;
    }
}

pub fn spawn(s: &Spec) -> Result<Child, String> {
    #[cfg(unix)]
    use std::os::unix::process::CommandExt;
    if let Some(d) = s.log.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let open = || {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&s.log)
            .map_err(|e| format!("{}: {e}", s.log.display()))
    };
    let (out, err) = (open()?, open()?);
    let mut c = Command::new(&s.program);
    c.args(&s.args)
        .current_dir(&s.cwd)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    for (k, v) in &s.env {
        c.env(k, v);
    }
    #[cfg(unix)]
    c.process_group(0);
    c.spawn()
        .map_err(|e| format!("{}: {} ({e})", s.name, s.program.display()))
}

/// `kill -<sig> <pid>`.
pub fn signal(pid: u32, sig: &str) -> Result<(), String> {
    let st = Command::new("kill")
        .arg(format!("-{sig}"))
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("kill: {e}"))?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("kill -{sig} {pid} failed"))
    }
}

/// Whether `pid` exists (`kill -0`).
pub fn alive(pid: u32) -> bool {
    signal(pid, "0").is_ok()
}

/// Stops a pid that is not our child (`down` after `up` exited): SIGINT,
/// then SIGKILL after `grace`.
pub fn stop_pid(pid: u32, grace: Duration) -> bool {
    if !alive(pid) {
        return false;
    }
    let _ = signal(pid, "INT");
    let t0 = Instant::now();
    while alive(pid) {
        if t0.elapsed() > grace {
            let _ = signal(pid, "KILL");
            std::thread::sleep(Duration::from_millis(200));
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    true
}

/// The command line of `pid` (`ps -o command=`), to check a recorded pid
/// still belongs to the component before signalling it.
pub fn command_of(pid: u32) -> Option<String> {
    let o = Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Whether a recorded component pid is still that component.
pub fn is_component(pid: u32, program: &Path) -> bool {
    let name = program
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    command_of(pid).is_some_and(|c| !name.is_empty() && c.contains(&name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_log_kill_and_finish() {
        let d = std::env::temp_dir().join(format!("psf-procs-{}", crate::run::wall_ms()));
        std::fs::create_dir_all(&d).unwrap();
        let mut p = Proc::new(Spec {
            name: "sleeper".into(),
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "echo hello; sleep 30".into()],
            env: vec![],
            cwd: d.clone(),
            log: d.join("logs/sleeper.log"),
            finishes: false,
        });
        let pid = p.start().unwrap();
        assert!(alive(pid));
        assert!(is_component(pid, Path::new("/bin/sh")));
        std::thread::sleep(Duration::from_millis(300));
        p.kill9().unwrap();
        assert!(!p.running());
        std::thread::sleep(Duration::from_millis(100));
        assert!(!alive(pid));
        assert!(std::fs::read_to_string(d.join("logs/sleeper.log"))
            .unwrap()
            .contains("hello"));
        let mut f = Proc::new(Spec {
            name: "done".into(),
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "exit 0".into()],
            env: vec![],
            cwd: d.clone(),
            log: d.join("logs/done.log"),
            finishes: true,
        });
        f.start().unwrap();
        let t0 = Instant::now();
        while f.poll_exit().is_none() && t0.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(f.done);
        let mut g = Proc::new(Spec {
            name: "int".into(),
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "sleep 30".into()],
            env: vec![],
            cwd: d.clone(),
            log: d.join("logs/int.log"),
            finishes: false,
        });
        let gp = g.start().unwrap();
        g.stop(Duration::from_secs(2));
        assert!(!alive(gp));
        let _ = std::fs::remove_dir_all(&d);
    }
}
