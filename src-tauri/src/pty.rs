//! One real pseudo-terminal per task, running the agent CLI inside the task's worktree.
//! Status is inferred without touching any agent config: the terminal title Claude sets
//! (✳ idle / spinner while working), permission prompts in the output, and output activity.
//! Idle agents can be put to sleep (process killed, conversation kept) and resumed with
//! the agent's own "continue" flag.
use anyhow::{anyhow, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const SCROLLBACK: usize = 256 * 1024; // bytes kept per task so switching tasks can repaint

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Working,
    Waiting, // "te necesita"
    Done,    // finished its turn
    Asleep,  // process stopped to save RAM, resumable
    Exited,
}

pub struct Agent {
    pub cmd: &'static str,
    pub resume: &'static [&'static str],
}

pub fn agent(name: &str) -> Agent {
    match name {
        "codex" => Agent { cmd: "codex", resume: &["resume", "--last"] },
        "copilot" => Agent { cmd: "copilot", resume: &["--continue"] },
        "opencode" => Agent { cmd: "opencode", resume: &["--continue"] },
        _ => Agent { cmd: "claude", resume: &["--continue"] },
    }
}

fn prompt_args(name: &str, prompt: &str) -> Vec<String> {
    if prompt.trim().is_empty() {
        return vec![];
    }
    match name {
        "copilot" => vec!["-i".into(), prompt.into()],
        "opencode" => vec!["--prompt".into(), prompt.into()],
        _ => vec![prompt.into()], // claude, codex: prompt as argument
    }
}

struct Session {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    agent: String,
    status: Status,
    title: String,
    last_output: Instant,
    last_busy: Instant, // last time the agent showed its "esc to interrupt" busy line
    scrollback: Vec<u8>,
}

#[derive(Clone, Default)]
pub struct Ptys {
    inner: Arc<Mutex<HashMap<String, Session>>>,
    asleep: Arc<Mutex<HashMap<String, String>>>, // task → agent, for resuming
}

#[derive(Serialize, Clone)]
struct DataEvent<'a> {
    id: &'a str,
    data: &'a str,
}

#[derive(Serialize, Clone)]
pub struct StatusEvent {
    pub id: String,
    pub status: Status,
    pub title: String,
}

impl Ptys {
    /// Start (or resume, when `resume`) the agent for a task.
    pub fn spawn(&self, app: &AppHandle, id: &str, cwd: &str, agent_name: &str, prompt: &str, model: &str, resume: bool) -> Result<()> {
        if self.inner.lock().unwrap().contains_key(id) {
            return Ok(()); // already running
        }
        let a = agent(agent_name);
        let mut args: Vec<String> = Vec::new();
        if resume {
            args.extend(a.resume.iter().map(|s| s.to_string()));
        }
        if !model.is_empty() && agent_name == "claude" {
            args.push("--model".into());
            args.push(model.into());
        }
        if !resume {
            args.extend(prompt_args(agent_name, prompt));
        }
        let mut program = vec![a.cmd.to_string()];
        program.extend(args);
        self.start(app, id, cwd, agent_name, build_cmd(cwd, id, Some(program), None))
    }

    /// Extra terminal for a task (id like "<task>#2"): a plain shell, or a command such as `npm run dev`.
    pub fn spawn_shell(&self, app: &AppHandle, id: &str, cwd: &str, command: &str) -> Result<()> {
        if self.inner.lock().unwrap().contains_key(id) {
            return Ok(());
        }
        let script = if command.trim().is_empty() { None } else { Some(command.to_string()) };
        self.start(app, id, cwd, "", build_cmd(cwd, id, None, script))
    }

    fn start(&self, app: &AppHandle, id: &str, cwd: &str, agent_name: &str, cmd: CommandBuilder) -> Result<()> {
        let _ = cwd;
        let pair = native_pty_system().openpty(PtySize { rows: 40, cols: 120, pixel_width: 0, pixel_height: 0 })?;
        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        self.asleep.lock().unwrap().remove(id);
        self.inner.lock().unwrap().insert(
            id.to_string(),
            Session {
                writer,
                master: pair.master,
                child,
                agent: agent_name.to_string(),
                status: if agent_name.is_empty() { Status::Done } else { Status::Working },
                title: String::new(),
                last_output: Instant::now(),
                last_busy: Instant::now(),
                scrollback: Vec::new(),
            },
        );
        self.emit_status(app, id);

        // Reader thread: forward output, keep scrollback, infer status.
        let me = self.clone();
        let app = app.clone();
        let id = id.to_string();
        std::thread::spawn(move || {
            let mut buf = [0u8; 16384];
            let mut pending: Vec<u8> = Vec::new(); // incomplete UTF-8 tail
            loop {
                let n = match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                pending.extend_from_slice(&buf[..n]);
                let valid = match std::str::from_utf8(&pending) {
                    Ok(_) => pending.len(),
                    Err(e) => e.valid_up_to(),
                };
                let text = String::from_utf8_lossy(&pending[..valid]).to_string();
                pending.drain(..valid);
                if text.is_empty() {
                    continue;
                }
                let changed = me.observe(&id, &text);
                let _ = app.emit("pty-data", DataEvent { id: &id, data: &text });
                if changed {
                    me.emit_status(&app, &id);
                }
            }
            // Process ended (or was put to sleep)
            let asleep = me.asleep.lock().unwrap().contains_key(&id);
            me.inner.lock().unwrap().remove(&id);
            let _ = app.emit(
                "pty-status",
                StatusEvent { id: id.clone(), status: if asleep { Status::Asleep } else { Status::Exited }, title: String::new() },
            );
        });
        Ok(())
    }

    /// Update scrollback/status from new output. Returns true when the status changed.
    fn observe(&self, id: &str, text: &str) -> bool {
        let mut map = self.inner.lock().unwrap();
        let Some(s) = map.get_mut(id) else { return false };
        s.scrollback.extend_from_slice(text.as_bytes());
        if s.scrollback.len() > SCROLLBACK {
            let cut = s.scrollback.len() - SCROLLBACK;
            s.scrollback.drain(..cut);
        }
        s.last_output = Instant::now();
        let before = s.status;
        // OSC 0/2 window title (shown in the UI; not trusted for status)
        let mut rest = text;
        while let Some(i) = rest.find("\x1b]") {
            rest = &rest[i + 2..];
            if let Some(end) = rest.find(|c| c == '\x07' || c == '\x1b') {
                let osc = &rest[..end];
                if let Some(t) = osc.strip_prefix("0;").or_else(|| osc.strip_prefix("2;")) {
                    s.title = t.trim_start_matches(|c: char| !c.is_alphanumeric()).to_string();
                }
                rest = &rest[end..];
            }
        }
        if s.agent.is_empty() {
            return false; // plain shells/dev servers: no agent status
        }
        let lower = text.to_lowercase();
        // Permission / question prompts → needs you
        if lower.contains("do you want to proceed") || lower.contains("do you want to make this edit")
            || lower.contains("do you want to create") || lower.contains("❯ 1. yes")
        {
            s.status = Status::Waiting;
        } else if lower.contains("esc to interrupt") || lower.contains("esc to cancel") || lower.contains("ctrl+c to interrupt") {
            // Claude/Codex/Copilot print this on their busy line while they work
            s.status = Status::Working;
            s.last_busy = Instant::now();
        } else if s.agent != "claude" && s.status != Status::Waiting {
            s.status = Status::Working;
            s.last_busy = Instant::now();
        }
        s.status != before
    }

    fn emit_status(&self, app: &AppHandle, id: &str) {
        if let Some(s) = self.inner.lock().unwrap().get(id) {
            let _ = app.emit("pty-status", StatusEvent { id: id.to_string(), status: s.status, title: s.title.clone() });
        }
    }

    pub fn write(&self, id: &str, data: &str) -> Result<()> {
        let mut map = self.inner.lock().unwrap();
        let s = map.get_mut(id).ok_or_else(|| anyhow!("la tarea no tiene terminal abierta"))?;
        s.writer.write_all(data.as_bytes())?;
        s.writer.flush()?;
        if s.status == Status::Waiting {
            s.status = Status::Working; // answered
        }
        Ok(())
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<()> {
        if let Some(s) = self.inner.lock().unwrap().get(id) {
            s.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })?;
        }
        Ok(())
    }

    pub fn scrollback(&self, id: &str) -> String {
        self.inner.lock().unwrap().get(id).map(|s| String::from_utf8_lossy(&s.scrollback).to_string()).unwrap_or_default()
    }

    pub fn statuses(&self) -> HashMap<String, (Status, String)> {
        let mut out: HashMap<String, (Status, String)> =
            self.inner.lock().unwrap().iter().filter(|(_, s)| !s.agent.is_empty()).map(|(k, s)| (k.clone(), (s.status, s.title.clone()))).collect();
        for k in self.asleep.lock().unwrap().keys() {
            out.entry(k.clone()).or_insert((Status::Asleep, String::new()));
        }
        out
    }

    /// Ids of tasks whose agent is alive (for restoring them as "asleep" next launch).
    pub fn running_agents(&self) -> Vec<(String, String)> {
        self.inner.lock().unwrap().iter().filter(|(_, s)| !s.agent.is_empty()).map(|(k, s)| (k.clone(), s.agent.clone())).collect()
    }

    pub fn mark_asleep(&self, id: &str, agent: &str) {
        self.asleep.lock().unwrap().insert(id.to_string(), agent.to_string());
    }

    pub fn kill_all(&self) {
        for s in self.inner.lock().unwrap().values_mut() {
            let _ = s.child.kill();
        }
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.inner.lock().unwrap().contains_key(id)
    }

    pub fn asleep_agent(&self, id: &str) -> Option<String> {
        self.asleep.lock().unwrap().get(id).cloned()
    }

    /// Stop the agent but remember it, so `spawn(.., resume=true)` brings the conversation back.
    pub fn sleep(&self, id: &str) {
        let mut map = self.inner.lock().unwrap();
        if let Some(s) = map.get_mut(id) {
            self.asleep.lock().unwrap().insert(id.to_string(), s.agent.clone());
            let _ = s.child.kill();
        }
    }

    pub fn kill(&self, id: &str) {
        self.asleep.lock().unwrap().remove(id);
        if let Some(s) = self.inner.lock().unwrap().get_mut(id) {
            let _ = s.child.kill();
        }
    }

    /// Every 3 s: agents whose busy line disappeared (Claude 4 s, others 8 s of silence) count as done; agents done for
    /// `sleep_after` minutes are put to sleep to free their RAM.
    pub fn start_ticker(&self, app: AppHandle, sleep_after: Arc<Mutex<u64>>) {
        let me = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(3));
            let mut to_sleep = Vec::new();
            let mut changed = Vec::new();
            {
                let mut map = me.inner.lock().unwrap();
                let mins = *sleep_after.lock().unwrap();
                for (id, s) in map.iter_mut() {
                    if s.agent.is_empty() {
                        continue;
                    }
                    let quiet = s.last_output.elapsed();
                    let idle_for = if s.agent == "claude" { 4 } else { 8 };
                    if s.status == Status::Working && s.last_busy.elapsed() > Duration::from_secs(idle_for) {
                        s.status = Status::Done;
                        changed.push(id.clone());
                    }
                    if mins > 0 && s.status == Status::Done && quiet > Duration::from_secs(mins * 60) {
                        to_sleep.push(id.clone());
                    }
                }
            }
            for id in changed {
                me.emit_status(&app, &id);
            }
            for id in to_sleep {
                me.sleep(&id);
            }
        });
    }
}

/// How a terminal is launched. Unix: through a login shell so the CLI finds its PATH, and when the
/// program exits the terminal stays usable as a shell. Windows: through PowerShell.
fn build_cmd(cwd: &str, id: &str, program: Option<Vec<String>>, script: Option<String>) -> CommandBuilder {
    let mut cmd;
    if cfg!(windows) {
        cmd = CommandBuilder::new("powershell.exe");
        cmd.arg("-NoLogo");
        cmd.arg("-NoExit");
        if let Some(p) = program {
            cmd.arg("-Command");
            let quoted: Vec<String> = p.iter().map(|a| format!("'{}'", a.replace('\'', "''"))).collect();
            cmd.arg(format!("& {}", quoted.join(" ")));
        } else if let Some(sc) = script {
            cmd.arg("-Command");
            cmd.arg(sc);
        }
    } else {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
        let bash = if std::path::Path::new("/bin/bash").exists() { "/bin/bash" } else { "/bin/sh" };
        cmd = CommandBuilder::new(bash);
        cmd.env("AGENTIA_SHELL", &shell);
        match (program, script) {
            (Some(p), _) => {
                cmd.args(["-lc", "\"$0\" \"$@\"; exec \"${AGENTIA_SHELL:-bash}\" -l"]);
                cmd.args(&p);
            }
            (None, Some(sc)) => {
                cmd.args(["-lc", &format!("{sc}; exec \"${{AGENTIA_SHELL:-bash}}\" -l")]);
            }
            (None, None) => {
                cmd.args(["-lc", "exec \"${AGENTIA_SHELL:-bash}\" -l"]);
            }
        }
    }
    cmd.cwd(cwd);
    // Don't leak the environment of whoever launched Agentia (e.g. a Claude Code session or Orca):
    // Claude would think it's a child session and stop saving its transcript.
    for (k, _) in std::env::vars() {
        if k.starts_with("CLAUDE_CODE") || k == "CLAUDECODE" || k.starts_with("ORCA_") || k.starts_with("AGENTIA_") {
            cmd.env_remove(&k);
        }
    }
    cmd.env("AGENTIA_TASK", id.split('#').next().unwrap_or(id));
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd
}
