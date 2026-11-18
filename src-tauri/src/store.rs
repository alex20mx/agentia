//! Agentia's own state (~/.config/agentia/state.json): projects and per-task metadata.
//! Worktrees themselves are discovered with git, so deleting a folder by hand never leaves ghosts.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub collapsed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TaskMeta {
    #[serde(default)]
    pub title: String, // human name ("Arreglar QR de WhatsApp")
    #[serde(default)]
    pub agent: String, // claude | codex | copilot | opencode
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub column: String, // por-hacer | trabajando | revision | hecho
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub last_activity: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    pub workspace_dir: String,
    pub branch_prefix: String,
    pub sleep_after_min: u64, // put idle agents to sleep after N minutes (0 = never)
    pub default_agent: String,
    #[serde(default = "default_editor")]
    pub editor: String, // command to open a task folder: code, zed, cursor…
}

fn default_editor() -> String {
    "code".into()
}

impl Default for Settings {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        let user = std::env::var("USER").unwrap_or_else(|_| "yo".into());
        Settings {
            workspace_dir: home.join("agentia").join("workspaces").to_string_lossy().into(),
            branch_prefix: user,
            sleep_after_min: 10,
            default_agent: "claude".into(),
            editor: default_editor(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct State {
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub tasks: BTreeMap<String, TaskMeta>, // key = worktree path
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub imported_from_orca: bool,
    #[serde(default)]
    pub running: Vec<String>, // agents open when the app closed → shown as asleep, resumed on click
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_default().join("agentia")
}

fn state_path() -> PathBuf {
    config_dir().join("state.json")
}

pub fn load() -> State {
    std::fs::read_to_string(state_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Atomic write (tmp + rename) so a crash never leaves a half-written file.
pub fn save(state: &State) -> Result<()> {
    std::fs::create_dir_all(config_dir())?;
    let tmp = state_path().with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(state)?)?;
    std::fs::rename(tmp, state_path())?;
    Ok(())
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Import projects (and task titles) from Orca's profile DB, read-only. Returns how many were added.
pub fn import_from_orca(state: &mut State) -> Result<usize> {
    let db = dirs::config_dir().unwrap_or_default().join("orca/profiles/local-default/profile-state.db");
    if !db.exists() {
        return Ok(0);
    }
    let uri = format!("file:{}?mode=ro", db.to_string_lossy());
    let conn = rusqlite::Connection::open_with_flags(
        uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )?;
    let get = |domain: &str| -> Option<serde_json::Value> {
        conn.query_row("select payload from profile_state_documents where domain = ?1", [domain], |r| r.get::<_, String>(0))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
    };
    let mut added = 0;
    if let Some(serde_json::Value::Array(repos)) = get("repos") {
        for r in repos {
            let path = r["path"].as_str().unwrap_or_default().to_string();
            if path.is_empty() || !PathBuf::from(&path).exists() || state.projects.iter().any(|p| p.path == path) {
                continue;
            }
            let name = r["displayName"].as_str().map(String::from).unwrap_or_else(|| {
                PathBuf::from(&path).file_name().map(|n| n.to_string_lossy().into()).unwrap_or_default()
            });
            state.projects.push(Project { id: r["id"].as_str().unwrap_or(&path).to_string(), name, path, collapsed: false });
            added += 1;
        }
    }
    // Task titles Orca gave its worktrees (key "<repoId>::<absPath>")
    if let Some(serde_json::Value::Object(meta)) = get("worktreeMeta") {
        for (key, m) in meta {
            if let Some((_, path)) = key.split_once("::") {
                let entry = state.tasks.entry(path.to_string()).or_default();
                if entry.title.is_empty() {
                    entry.title = m["displayName"].as_str().unwrap_or_default().to_string();
                }
                if entry.column.is_empty() {
                    entry.column = match m["workspaceStatus"].as_str().unwrap_or("") {
                        "todo" => "por-hacer",
                        "in-review" => "revision",
                        "completed" => "hecho",
                        _ => "trabajando",
                    }
                    .into();
                }
            }
        }
    }
    if let Some(s) = get("settings") {
        if let Some(dir) = s["workspaceDir"].as_str() {
            state.settings.workspace_dir = dir.to_string();
        }
    }
    state.imported_from_orca = true;
    Ok(added)
}
