// Typed bridge to the Rust side (src-tauri/src/lib.rs).
import { invoke } from "@tauri-apps/api/core";

export type Status = "working" | "waiting" | "done" | "asleep" | "exited" | "idle";

export interface Task {
  path: string;
  branch: string;
  is_main: boolean;
  title: string;
  agent: string;
  model: string;
  column: string;
  status: Status;
  term_title: string;
  created_at: number;
  last_activity: number;
  session: { text: string; at: number } | null;
}

export interface Project {
  id: string;
  name: string;
  path: string;
  collapsed: boolean;
  error: string;
  icon: string; // data URL, "shopify", or "" (folder)
  tasks: Task[];
}

export interface Settings {
  workspace_dir: string;
  branch_prefix: string;
  sleep_after_min: number;
  default_agent: string;
  editor: string;
}

export interface Snapshot {
  projects: Project[];
  settings: Settings;
}

export interface FileChange { path: string; status: string; added: number; removed: number; binary: boolean; }
export interface FileContent { text: string; binary: boolean; size: number; truncated: boolean; }
export interface Changes { base: string; files: FileChange[]; added: number; removed: number; }

export interface Todo { content: string; status: "pending" | "in_progress" | "completed"; }
export interface Activity { kind: string; text: string; ts: string; }
export interface Insight { todos: Todo[]; activity: Activity[]; session: string; }
export interface UsageWindow { label: string; used: number; resets_at: number; }
export interface ProviderUsage { provider: string; plan: string; windows: UsageWindow[]; error: string; }
export interface PrInfo { number: number; url: string; state: string; title: string; }

export const api = {
  snapshot: () => invoke<Snapshot>("get_snapshot"),
  importOrca: () => invoke<number>("import_orca"),
  addProject: (path: string) => invoke<void>("add_project", { path }),
  setCollapsed: (id: string, collapsed: boolean) => invoke<void>("set_collapsed", { id, collapsed }),
  preview: (projectId: string, title: string) =>
    invoke<{ path: string; branch: string }>("preview_task", { projectId, title }),
  defaultBase: (projectId: string) => invoke<string>("default_base", { projectId }),
  createTask: (task: {
    project_id: string; title: string; agent: string; model: string; prompt: string;
    base: string; copy_env: boolean; start: boolean;
  }) => invoke<{ path: string; branch: string; copied: string[] }>("create_task", { task }),
  openTask: (path: string, start = true, fresh = false) => invoke<string>("open_task", { path, fresh, start }),
  openTerminal: (id: string, cwd: string, command = "") => invoke<string>("open_terminal", { id, cwd, command }),
  scripts: (path: string) => invoke<string[]>("task_scripts", { path }),
  listFiles: (path: string) => invoke<string[]>("list_files", { path }),
  readFile: (path: string, file: string) => invoke<FileContent>("read_file", { path, file }),
  fileDiff: (path: string, file: string) => invoke<string>("file_diff", { path, file }),
  commit: (path: string, message: string) => invoke<string>("commit_all", { path, message }),
  discard: (path: string, file: string) => invoke<void>("discard_file", { path, file }),
  uncommitted: (path: string) => invoke<number>("uncommitted", { path }),
  push: (path: string) => invoke<string>("push", { path }),
  prInfo: (path: string) => invoke<PrInfo | null>("pr_info", { path }),
  createPr: (path: string, title: string, body: string, draft: boolean) =>
    invoke<PrInfo>("create_pr", { path, title, body, draft }),
  insight: (path: string) => invoke<Insight>("task_insight", { path }),
  usage: () => invoke<ProviderUsage[]>("plan_usage"),
  openInEditor: (path: string) => invoke<void>("open_in_editor", { path }),
  rename: (path: string, title: string) => invoke<void>("rename_task", { path, title }),
  removeProject: (id: string) => invoke<void>("remove_project", { id }),
  write: (id: string, data: string) => invoke<void>("pty_write", { id, data }),
  resize: (id: string, cols: number, rows: number) => invoke<void>("pty_resize", { id, cols, rows }),
  sleep: (path: string) => invoke<void>("sleep_task", { path }),
  stop: (path: string) => invoke<void>("stop_task", { path }),
  changes: (path: string) => invoke<Changes>("task_changes", { path }),
  setColumn: (path: string, column: string) => invoke<void>("set_column", { path, column }),
  deleteTask: (projectId: string, path: string, deleteBranch: boolean) =>
    invoke<void>("delete_task", { projectId, path, deleteBranch }),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
};

export const PROVIDERS: Record<string, { name: string; glyph: string; color: string }> = {
  claude: { name: "Claude", glyph: "✳", color: "var(--claude)" },
  codex: { name: "Codex", glyph: "◎", color: "var(--codex)" },
  antigravity: { name: "Antigravity", glyph: "▲", color: "#4f8cff" },
};

export function untilReset(ts: number): string {
  if (!ts) return "";
  const m = Math.round((ts - Date.now() / 1000) / 60);
  if (m <= 0) return "ya";
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  if (h < 48) return `${h} h ${m % 60} min`;
  return `${Math.round(h / 24)} días`;
}

export const AGENTS: Record<string, { name: string; glyph: string; color: string; models: string[] }> = {
  claude: { name: "Claude Code", glyph: "✳", color: "var(--claude)", models: ["", "opus", "sonnet", "haiku"] },
  codex: { name: "Codex", glyph: "◎", color: "var(--codex)", models: [""] },
  copilot: { name: "Copilot CLI", glyph: "◆", color: "var(--copilot)", models: [""] },
  opencode: { name: "OpenCode", glyph: "▣", color: "var(--opencode)", models: [""] },
};

export const STATUS: Record<Status, { label: string; color: string }> = {
  working: { label: "trabajando", color: "var(--working)" },
  waiting: { label: "te necesita", color: "var(--waiting)" },
  done: { label: "listo", color: "var(--done)" },
  asleep: { label: "dormido", color: "var(--paused)" },
  exited: { label: "terminado", color: "var(--paused)" },
  idle: { label: "sin agente", color: "var(--text3)" },
};

export function ago(sec: number): string {
  if (!sec) return "";
  const m = Math.floor((Date.now() / 1000 - sec) / 60);
  if (m < 1) return "ahora";
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} h`;
  const d = Math.floor(h / 24);
  return d === 1 ? "ayer" : `${d} d`;
}
