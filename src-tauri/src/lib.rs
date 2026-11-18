//! Agentia — run many coding agents (Claude Code, Codex, Copilot, OpenCode) in parallel,
//! one git worktree per task. No API keys: it only launches the official CLIs, which use
//! the logins you already have.
mod git;
mod insight;
mod pty;
mod store;
mod usage;

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};

struct App {
    state: Mutex<store::State>,
    ptys: pty::Ptys,
    sleep_after: Arc<Mutex<u64>>,
}

type Res<T> = Result<T, String>;
fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[derive(Serialize)]
struct TaskView {
    path: String,
    branch: String,
    is_main: bool,
    title: String,
    agent: String,
    model: String,
    column: String,
    status: String, // working | waiting | done | asleep | exited | idle
    term_title: String,
    created_at: u64,
    last_activity: u64,
    session: Option<insight::SessionLine>,
}

#[derive(Serialize)]
struct ProjectView {
    id: String,
    name: String,
    path: String,
    collapsed: bool,
    error: String,
    icon: String,
    tasks: Vec<TaskView>,
}

#[derive(Serialize)]
struct Snapshot {
    projects: Vec<ProjectView>,
    settings: store::Settings,
}

fn snapshot(app: &App) -> Snapshot {
    let st = app.state.lock().unwrap().clone();
    let statuses = app.ptys.statuses();
    let projects = st
        .projects
        .iter()
        .map(|p| {
            let (wts, error) = match git::list_worktrees(Path::new(&p.path)) {
                Ok(w) => (w, String::new()),
                Err(e) => (vec![], e.to_string()),
            };
            let tasks = wts
                .into_iter()
                .map(|w| {
                    let meta = st.tasks.get(&w.path).cloned().unwrap_or_default();
                    let session = insight::session_line(Path::new(&w.path));
                    let (status, term_title) = statuses
                        .get(&w.path)
                        .map(|(s, t)| (serde_json::to_value(s).unwrap().as_str().unwrap_or("idle").to_string(), t.clone()))
                        .unwrap_or_else(|| ("idle".into(), String::new()));
                    TaskView {
                        title: if meta.title.is_empty() { w.branch.rsplit('/').next().unwrap_or(&w.branch).to_string() } else { meta.title },
                        path: w.path,
                        branch: w.branch,
                        is_main: w.is_main,
                        agent: meta.agent,
                        model: meta.model,
                        column: if meta.column.is_empty() { "trabajando".into() } else { meta.column },
                        status,
                        term_title,
                        created_at: meta.created_at,
                        last_activity: meta.last_activity,
                        session,
                    }
                })
                .collect();
            let icon = insight::project_icon(Path::new(&p.path));
            ProjectView { id: p.id.clone(), name: p.name.clone(), path: p.path.clone(), collapsed: p.collapsed, error, icon, tasks }
        })
        .collect();
    Snapshot { projects, settings: st.settings }
}

#[tauri::command]
fn get_snapshot(app: State<App>) -> Snapshot {
    snapshot(&app)
}

#[tauri::command]
fn import_orca(app: State<App>) -> Res<usize> {
    let mut st = app.state.lock().unwrap();
    let n = store::import_from_orca(&mut st).map_err(err)?;
    store::save(&st).map_err(err)?;
    Ok(n)
}

#[tauri::command]
fn add_project(app: State<App>, path: String) -> Res<()> {
    let p = PathBuf::from(&path);
    git::git(&p, &["rev-parse", "--git-dir"]).map_err(|_| "Esa carpeta no es un repositorio git".to_string())?;
    let mut st = app.state.lock().unwrap();
    if !st.projects.iter().any(|x| x.path == path) {
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(path.clone());
        st.projects.push(store::Project { id: path.clone(), name, path, collapsed: false });
        store::save(&st).map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
fn set_collapsed(app: State<App>, id: String, collapsed: bool) -> Res<()> {
    let mut st = app.state.lock().unwrap();
    if let Some(p) = st.projects.iter_mut().find(|p| p.id == id) {
        p.collapsed = collapsed;
    }
    store::save(&st).map_err(err)
}

#[derive(serde::Deserialize)]
struct NewTask {
    project_id: String,
    title: String,
    agent: String,
    model: String,
    prompt: String,
    base: String,
    copy_env: bool,
    start: bool,
}

#[derive(Serialize)]
struct Created {
    path: String,
    branch: String,
    copied: Vec<String>,
}

#[tauri::command]
fn preview_task(app: State<App>, project_id: String, title: String) -> Res<Created> {
    let st = app.state.lock().unwrap();
    let p = st.projects.iter().find(|p| p.id == project_id).ok_or("Proyecto no encontrado")?;
    let slug = git::slug(&title);
    let branch = format!("{}/{}", st.settings.branch_prefix, slug);
    let path = Path::new(&st.settings.workspace_dir).join(&p.name).join(&slug);
    Ok(Created { path: path.to_string_lossy().into(), branch, copied: vec![] })
}

#[tauri::command]
fn default_base(app: State<App>, project_id: String) -> Res<String> {
    let st = app.state.lock().unwrap();
    let p = st.projects.iter().find(|p| p.id == project_id).ok_or("Proyecto no encontrado")?;
    Ok(git::default_base(Path::new(&p.path)))
}

#[tauri::command]
fn create_task(handle: AppHandle, app: State<App>, task: NewTask) -> Res<Created> {
    let (repo, path, branch, copied) = {
        let st = app.state.lock().unwrap();
        let p = st.projects.iter().find(|p| p.id == task.project_id).ok_or("Proyecto no encontrado")?;
        let slug = git::slug(&task.title);
        let branch = format!("{}/{}", st.settings.branch_prefix, slug);
        let path = Path::new(&st.settings.workspace_dir).join(&p.name).join(&slug);
        if path.exists() {
            return Err(format!("Ya existe una tarea en {}", path.display()));
        }
        let repo = PathBuf::from(&p.path);
        let base = if task.base.is_empty() { git::default_base(&repo) } else { task.base.clone() };
        git::create_worktree(&repo, &path, &branch, &base).map_err(err)?;
        let copied = if task.copy_env { git::copy_includes(&repo, &path) } else { vec![] };
        (repo, path, branch, copied)
    };
    let _ = repo;
    let key = path.to_string_lossy().to_string();
    {
        let mut st = app.state.lock().unwrap();
        st.tasks.insert(
            key.clone(),
            store::TaskMeta {
                title: task.title.clone(),
                agent: task.agent.clone(),
                model: task.model.clone(),
                column: if task.start { "trabajando".into() } else { "por-hacer".into() },
                created_at: store::now(),
                last_activity: store::now(),
            },
        );
        store::save(&st).map_err(err)?;
    }
    if task.start {
        app.ptys.spawn(&handle, &key, &key, &task.agent, &task.prompt, &task.model, false).map_err(err)?;
    }
    Ok(Created { path: key, branch, copied })
}

/// Open a task's terminal: running → nothing to do; asleep or not started → start/resume its agent.
#[tauri::command]
fn open_task(handle: AppHandle, app: State<App>, path: String, fresh: bool, start: bool) -> Res<String> {
    if app.ptys.is_running(&path) {
        return Ok(app.ptys.scrollback(&path));
    }
    if !start {
        return Ok(String::new()); // e.g. a project's main checkout: don't launch an agent by itself
    }
    let meta = app.state.lock().unwrap().tasks.get(&path).cloned().unwrap_or_default();
    let agent = app.ptys.asleep_agent(&path).unwrap_or(if meta.agent.is_empty() { "claude".into() } else { meta.agent.clone() });
    app.ptys.spawn(&handle, &path, &path, &agent, "", &meta.model, !fresh).map_err(err)?;
    {
        let mut st = app.state.lock().unwrap();
        let e = st.tasks.entry(path.clone()).or_default();
        e.agent = agent;
        e.last_activity = store::now();
        let _ = store::save(&st);
    }
    Ok(String::new())
}

#[tauri::command]
fn pty_write(app: State<App>, id: String, data: String) -> Res<()> {
    app.ptys.write(&id, &data).map_err(err)
}

#[tauri::command]
fn pty_resize(app: State<App>, id: String, cols: u16, rows: u16) -> Res<()> {
    app.ptys.resize(&id, cols, rows).map_err(err)
}

#[tauri::command]
fn sleep_task(app: State<App>, path: String) {
    app.ptys.sleep(&path);
}

#[tauri::command]
fn stop_task(app: State<App>, path: String) {
    app.ptys.kill(&path);
}

#[tauri::command]
fn task_changes(path: String) -> Res<git::Changes> {
    git::changes(Path::new(&path)).map_err(err)
}

#[tauri::command]
fn set_column(app: State<App>, path: String, column: String) -> Res<()> {
    let mut st = app.state.lock().unwrap();
    st.tasks.entry(path).or_default().column = column;
    store::save(&st).map_err(err)
}

/// Remove a task: stop its agent, `git worktree remove`, optionally delete its branch.
#[tauri::command]
fn delete_task(app: State<App>, project_id: String, path: String, delete_branch: bool) -> Res<()> {
    app.ptys.kill(&path);
    let repo = {
        let st = app.state.lock().unwrap();
        PathBuf::from(&st.projects.iter().find(|p| p.id == project_id).ok_or("Proyecto no encontrado")?.path)
    };
    let branch = git::git(Path::new(&path), &["rev-parse", "--abbrev-ref", "HEAD"]).map(|s| s.trim().to_string()).unwrap_or_default();
    git::git(&repo, &["worktree", "remove", "--", &path]).map_err(err)?;
    if delete_branch && !branch.is_empty() && branch != "HEAD" {
        let _ = git::git(&repo, &["branch", "-D", "--", &branch]);
    }
    let mut st = app.state.lock().unwrap();
    st.tasks.remove(&path);
    store::save(&st).map_err(err)
}

#[tauri::command]
fn save_settings(app: State<App>, settings: store::Settings) -> Res<()> {
    *app.sleep_after.lock().unwrap() = settings.sleep_after_min;
    let mut st = app.state.lock().unwrap();
    st.settings = settings;
    store::save(&st).map_err(err)
}


#[tauri::command]
fn open_terminal(handle: AppHandle, app: State<App>, id: String, cwd: String, command: String) -> Res<String> {
    if app.ptys.is_running(&id) {
        return Ok(app.ptys.scrollback(&id));
    }
    app.ptys.spawn_shell(&handle, &id, &cwd, &command).map_err(err)?;
    Ok(String::new())
}

/// `npm run …` scripts of the task (package.json), for the "run" tab menu.
#[tauri::command]
fn task_scripts(path: String) -> Vec<String> {
    let Ok(t) = std::fs::read_to_string(Path::new(&path).join("package.json")) else { return vec![] };
    let v: serde_json::Value = serde_json::from_str(&t).unwrap_or_default();
    let pm = if Path::new(&path).join("pnpm-lock.yaml").exists() { "pnpm" }
        else if Path::new(&path).join("bun.lockb").exists() || Path::new(&path).join("bun.lock").exists() { "bun" }
        else if Path::new(&path).join("yarn.lock").exists() { "yarn" } else { "npm" };
    v["scripts"].as_object().map(|o| o.keys().map(|k| format!("{pm} run {k}")).collect()).unwrap_or_default()
}

#[tauri::command]
fn list_files(path: String) -> Res<Vec<String>> {
    git::list_files(Path::new(&path)).map_err(err)
}

#[tauri::command]
fn read_file(path: String, file: String) -> Res<git::FileContent> {
    git::read_file(Path::new(&path), &file).map_err(err)
}

#[tauri::command]
fn file_diff(path: String, file: String) -> Res<String> {
    git::file_diff(Path::new(&path), &file).map_err(err)
}

#[tauri::command]
fn commit_all(path: String, message: String) -> Res<String> {
    git::commit_all(Path::new(&path), &message).map_err(err)
}

#[tauri::command]
fn discard_file(path: String, file: String) -> Res<()> {
    git::discard_file(Path::new(&path), &file).map_err(err)
}

#[tauri::command]
fn uncommitted(path: String) -> usize {
    git::uncommitted(Path::new(&path))
}

#[tauri::command]
async fn push(path: String) -> Res<String> {
    tauri::async_runtime::spawn_blocking(move || git::push(Path::new(&path)).map_err(err)).await.map_err(err)?
}

#[tauri::command]
async fn pr_info(path: String) -> Option<git::PrInfo> {
    tauri::async_runtime::spawn_blocking(move || git::pr_info(Path::new(&path))).await.ok().flatten()
}

#[tauri::command]
async fn create_pr(path: String, title: String, body: String, draft: bool) -> Res<git::PrInfo> {
    tauri::async_runtime::spawn_blocking(move || git::create_pr(Path::new(&path), &title, &body, draft).map_err(err)).await.map_err(err)?
}

#[tauri::command]
fn task_insight(path: String) -> insight::Insight {
    insight::read(Path::new(&path))
}

#[tauri::command]
async fn plan_usage() -> Vec<usage::ProviderUsage> {
    tauri::async_runtime::spawn_blocking(usage::all).await.unwrap_or_default()
}

#[tauri::command]
fn open_in_editor(app: State<App>, path: String) -> Res<()> {
    let editor = app.state.lock().unwrap().settings.editor.clone();
    let bin = usage::which(&editor).unwrap_or_else(|| PathBuf::from(&editor));
    let mut cmd = if cfg!(windows) {
        // editors install as .cmd shims on Windows (code.cmd, cursor.cmd…): run them through cmd
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(&bin);
        c
    } else if cfg!(target_os = "macos") && usage::which(&editor).is_none() {
        // macOS: fall back to the app bundle ("Visual Studio Code", "Zed"…) via `open -a`
        let mut c = std::process::Command::new("open");
        c.args(["-a", &editor]);
        c
    } else {
        std::process::Command::new(bin)
    };
    cmd.arg(&path).spawn().map(|_| ()).map_err(|_| format!("No encontré el editor \"{editor}\". Cámbialo en Ajustes."))
}

#[tauri::command]
fn rename_task(app: State<App>, path: String, title: String) -> Res<()> {
    let mut st = app.state.lock().unwrap();
    st.tasks.entry(path).or_default().title = title;
    store::save(&st).map_err(err)
}

#[tauri::command]
fn remove_project(app: State<App>, id: String) -> Res<()> {
    let mut st = app.state.lock().unwrap();
    st.projects.retain(|p| p.id != id);
    store::save(&st).map_err(err)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut st = store::load();
    if !st.imported_from_orca && st.projects.is_empty() {
        let _ = store::import_from_orca(&mut st); // first launch: bring your Orca projects
        let _ = store::save(&st);
    }
    let sleep_after = Arc::new(Mutex::new(st.settings.sleep_after_min));
    let ptys = pty::Ptys::default();
    // Agents that were open last time come back as "asleep": one click resumes their conversation
    for id in std::mem::take(&mut st.running) {
        let agent = st.tasks.get(&id).map(|t| t.agent.clone()).filter(|a| !a.is_empty()).unwrap_or_else(|| "claude".into());
        ptys.mark_asleep(&id, &agent);
    }
    let app = App { state: Mutex::new(st), ptys: ptys.clone(), sleep_after: sleep_after.clone() };
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(app)
        .setup(move |a| {
            ptys.start_ticker(a.handle().clone(), sleep_after.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            import_orca,
            add_project,
            set_collapsed,
            preview_task,
            default_base,
            create_task,
            open_task,
            pty_write,
            pty_resize,
            sleep_task,
            stop_task,
            task_changes,
            set_column,
            delete_task,
            save_settings,
            open_terminal,
            task_scripts,
            file_diff,
            commit_all,
            discard_file,
            uncommitted,
            push,
            pr_info,
            create_pr,
            task_insight,
            plan_usage,
            open_in_editor,
            rename_task,
            remove_project,
            list_files,
            read_file
        ])
        .build(tauri::generate_context!())
        .expect("no se pudo iniciar Agentia")
        .run(|handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                // Remember which agents were open, then stop them cleanly (their conversations are saved)
                let app = handle.state::<App>();
                let mut ids: Vec<String> = app.ptys.running_agents().into_iter().map(|(id, _)| id).collect();
                ids.extend(app.ptys.statuses().into_iter().filter(|(_, (s, _))| *s == pty::Status::Asleep).map(|(id, _)| id));
                ids.sort();
                ids.dedup();
                let mut st = app.state.lock().unwrap();
                st.running = ids;
                let _ = store::save(&st);
                drop(st);
                app.ptys.kill_all();
            }
        });
}
