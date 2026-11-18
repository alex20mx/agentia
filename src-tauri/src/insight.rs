//! "Progreso" and "Actividad" for a task, read from the agent's own session log — no hooks needed.
//! Claude Code writes every session to ~/.claude/projects/<cwd with non-alphanumerics as '-'>/<id>.jsonl;
//! we read the tail of the newest one: its latest TodoWrite list = progress, tool calls = activity.
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Default)]
pub struct Todo {
    pub content: String,
    pub status: String, // pending | in_progress | completed
}

#[derive(Serialize, Clone, Default)]
pub struct Activity {
    pub kind: String, // read | edit | run | search | web | agent | other
    pub text: String,
    pub ts: String,
}

#[derive(Serialize, Clone, Default)]
pub struct Insight {
    pub todos: Vec<Todo>,
    pub activity: Vec<Activity>,
    pub session: String,
}

fn claude_dir(cwd: &Path) -> PathBuf {
    let enc: String = cwd.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    dirs::home_dir().unwrap_or_default().join(".claude/projects").join(enc)
}

fn newest_jsonl(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "jsonl").unwrap_or(false))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

fn short(s: &str, n: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() > n { format!("{}…", s.chars().take(n).collect::<String>()) } else { s }
}

fn file_name(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.to_string())
}

pub fn read(cwd: &Path) -> Insight {
    let mut out = Insight::default();
    let Some(file) = newest_jsonl(&claude_dir(cwd)) else { return out };
    out.session = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    // Only the last ~600 KB: enough for the latest todos and recent activity
    let Ok(mut f) = std::fs::File::open(&file) else { return out };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(600 * 1024);
    let _ = f.seek(SeekFrom::Start(start));
    let mut buf = String::new();
    let _ = f.read_to_string(&mut buf);
    let mut lines: Vec<&str> = buf.lines().collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0); // probably cut in half
    }
    for line in lines {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if v["type"] != "assistant" {
            continue;
        }
        let ts = v["timestamp"].as_str().unwrap_or_default().to_string();
        let Some(items) = v["message"]["content"].as_array() else { continue };
        for it in items {
            if it["type"] != "tool_use" {
                continue;
            }
            let name = it["name"].as_str().unwrap_or_default();
            let inp = &it["input"];
            if name == "TodoWrite" {
                if let Some(todos) = inp["todos"].as_array() {
                    out.todos = todos
                        .iter()
                        .map(|t| Todo {
                            content: t["content"].as_str().or(t["activeForm"].as_str()).unwrap_or_default().to_string(),
                            status: t["status"].as_str().unwrap_or("pending").to_string(),
                        })
                        .collect();
                }
                continue;
            }
            let (kind, text) = match name {
                "Read" => ("read", format!("Leyó {}", file_name(inp["file_path"].as_str().unwrap_or("un archivo")))),
                "Edit" | "MultiEdit" => ("edit", format!("Editó {}", file_name(inp["file_path"].as_str().unwrap_or("un archivo")))),
                "Write" => ("edit", format!("Creó {}", file_name(inp["file_path"].as_str().unwrap_or("un archivo")))),
                "Bash" => ("run", format!("Ejecutó {}", short(inp["command"].as_str().unwrap_or("un comando"), 60))),
                "Grep" | "Glob" => ("search", format!("Buscó {}", short(inp["pattern"].as_str().unwrap_or(""), 40))),
                "WebFetch" | "WebSearch" => ("web", "Buscó en internet".to_string()),
                "Task" | "Agent" => ("agent", format!("Lanzó un subagente: {}", short(inp["description"].as_str().unwrap_or(""), 40))),
                n => ("other", format!("Usó {n}")),
            };
            out.activity.push(Activity { kind: kind.into(), text, ts: ts.clone() });
        }
    }
    let n = out.activity.len();
    if n > 14 {
        out.activity.drain(..n - 14);
    }
    out.activity.reverse(); // newest first
    out
}

/// One-line summary of a task's latest agent session for the sidebar: the agent's last words + when.
#[derive(Serialize, Clone, Default)]
pub struct SessionLine {
    pub text: String,
    pub at: u64, // epoch seconds of the last write
}

pub fn session_line(cwd: &Path) -> Option<SessionLine> {
    let file = newest_jsonl(&claude_dir(cwd))?;
    let at = file.metadata().ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let mut f = std::fs::File::open(&file).ok()?;
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let _ = f.seek(SeekFrom::Start(len.saturating_sub(64 * 1024)));
    let mut buf = String::new();
    let _ = f.read_to_string(&mut buf);
    let mut text = String::new();
    for line in buf.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if v["type"] == "summary" {
            if let Some(s) = v["summary"].as_str() {
                text = s.to_string();
            }
        }
        if v["type"] != "assistant" {
            continue;
        }
        for it in v["message"]["content"].as_array().into_iter().flatten() {
            if it["type"] == "text" {
                if let Some(t) = it["text"].as_str() {
                    let t = t.trim();
                    if !t.is_empty() {
                        text = t.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim_start_matches(['#', '*', ' ']).to_string();
                    }
                }
            }
        }
    }
    Some(SessionLine { text: short(&text, 70), at })
}

/// A project's own icon: favicon / logo found in the repo (as a data URL), or "shopify" for themes/apps.
pub fn project_icon(repo: &Path) -> String {
    const CANDIDATES: &[&str] = &[
        "public/favicon.svg", "public/logo.svg", "public/icon.svg", "app/icon.svg", "src/app/icon.svg", "static/favicon.svg",
        "public/favicon.png", "public/apple-touch-icon.png", "public/logo.png", "public/icon.png", "app/icon.png",
        "src/app/icon.png", "static/favicon.png", "assets/logo.png", "assets/icon.png", "logo.png", "icon.png",
        "public/favicon.ico", "app/favicon.ico", "src/app/favicon.ico", "favicon.ico",
    ];
    for c in CANDIDATES {
        let p = repo.join(c);
        let Ok(meta) = p.metadata() else { continue };
        if !meta.is_file() || meta.len() > 150_000 {
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else { continue };
        let mime = if c.ends_with(".svg") { "image/svg+xml" } else if c.ends_with(".ico") { "image/x-icon" } else { "image/png" };
        return format!("data:{mime};base64,{}", b64(&bytes));
    }
    let shopify = ["shopify.app.toml", "config/settings_schema.json", "layout/theme.liquid", ".shopify"]
        .iter()
        .any(|f| repo.join(f).exists());
    if shopify { "shopify".into() } else { String::new() }
}

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}
