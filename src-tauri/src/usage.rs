//! Plan usage per provider (like Orca): how much of the 5-hour / weekly limits you've used.
//! Credentials never leave your machine except to each provider's own official API, read-only.
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

#[derive(Serialize, Clone, Default)]
pub struct Window {
    pub label: String,   // "5 h", "Semana", "Opus semanal"…
    pub used: f64,       // 0–100
    pub resets_at: i64,  // epoch seconds (0 = unknown)
}

#[derive(Serialize, Clone, Default)]
pub struct ProviderUsage {
    pub provider: String, // claude | codex | antigravity
    pub plan: String,
    pub windows: Vec<Window>,
    pub error: String,
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(12))).build().into()
}

fn pct(v: &serde_json::Value) -> Option<f64> {
    v["used_percentage"].as_f64().or(v["utilization"].as_f64()).or(v["used_percent"].as_f64())
}

fn epoch(v: &serde_json::Value) -> i64 {
    if let Some(n) = v.as_i64() {
        return n;
    }
    if let Some(s) = v.as_str() {
        // RFC3339 → epoch (good enough parser: YYYY-MM-DDTHH:MM:SS)
        let p: Vec<i64> = s.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).take(6).filter_map(|x| x.parse().ok()).collect();
        if p.len() == 6 {
            let (y, m, d) = (p[0], p[1], p[2]);
            let (y2, m2) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
            let days = 365 * y2 + y2 / 4 - y2 / 100 + y2 / 400 + (m2 * 306 + 5) / 10 + (d - 1) - 719468;
            return days * 86400 + p[3] * 3600 + p[4] * 60 + p[5];
        }
    }
    0
}

/// Claude: GET api.anthropic.com/api/oauth/usage with the token Claude Code already stored.
pub fn claude() -> Option<ProviderUsage> {
    let dir = std::env::var("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|_| home().join(".claude"));
    let creds: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join(".credentials.json")).ok()?).ok()?;
    let oauth = &creds["claudeAiOauth"];
    let token = oauth["accessToken"].as_str()?;
    let mut out = ProviderUsage { provider: "claude".into(), plan: oauth["subscriptionType"].as_str().unwrap_or("").into(), ..Default::default() };
    let res = agent()
        .get("https://api.anthropic.com/api/oauth/usage")
        .header("Authorization", &format!("Bearer {token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("User-Agent", "claude-code/2.1.0")
        .call();
    match res {
        Ok(mut r) => {
            let v: serde_json::Value = r.body_mut().read_json().unwrap_or_default();
            for (key, label) in [("five_hour", "5 h"), ("seven_day", "Semana"), ("seven_day_opus", "Opus semana"), ("seven_day_sonnet", "Sonnet semana")] {
                if let Some(u) = pct(&v[key]) {
                    out.windows.push(Window { label: label.into(), used: u, resets_at: epoch(&v[key]["resets_at"]) });
                }
            }
            if let Some(limits) = v["limits"].as_array() {
                for l in limits {
                    if let (Some(p), Some(name)) = (l["percent"].as_f64(), l["scope"]["model"]["display_name"].as_str()) {
                        out.windows.push(Window { label: format!("{name} semanal"), used: p, resets_at: epoch(&l["resets_at"]) });
                    }
                }
            }
        }
        Err(ureq::Error::StatusCode(401)) => out.error = "Sesión vencida: abre Claude una vez para renovarla".into(),
        Err(ureq::Error::StatusCode(429)) => out.error = "Demasiadas consultas, reintento luego".into(),
        Err(e) => out.error = format!("Sin conexión ({e})"),
    }
    Some(out)
}

/// Codex: chatgpt.com/backend-api/wham/usage with ~/.codex/auth.json (only if Codex is set up).
pub fn codex() -> Option<ProviderUsage> {
    let dir = std::env::var("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|_| home().join(".codex"));
    let auth: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("auth.json")).ok()?).ok()?;
    let token = auth["tokens"]["access_token"].as_str()?;
    let account = auth["tokens"]["account_id"].as_str().unwrap_or("");
    let mut out = ProviderUsage { provider: "codex".into(), ..Default::default() };
    match agent()
        .get("https://chatgpt.com/backend-api/wham/usage")
        .header("Authorization", &format!("Bearer {token}"))
        .header("ChatGPT-Account-Id", account)
        .header("User-Agent", "codex-cli")
        .header("OpenAI-Beta", "codex-1")
        .call()
    {
        Ok(mut r) => {
            let v: serde_json::Value = r.body_mut().read_json().unwrap_or_default();
            out.plan = v["plan_type"].as_str().unwrap_or("").into();
            for (key, label) in [("primary_window", "5 h"), ("secondary_window", "Semana")] {
                let w = &v["rate_limit"][key];
                if let Some(u) = pct(w) {
                    out.windows.push(Window { label: label.into(), used: u, resets_at: epoch(&w["reset_at"]) });
                }
            }
        }
        Err(e) => out.error = format!("No se pudo consultar ({e})"),
    }
    Some(out)
}

/// Antigravity: its own CLI reports usage (`agy -p /usage --output-format json`).
pub fn antigravity() -> Option<ProviderUsage> {
    let agy = which("agy")?;
    let out = Command::new(&agy)
        .args(["-p", "/usage", "--output-format", "json", "--print-timeout", "20s"])
        .env("AGY_CLI_DISABLE_AUTO_UPDATE", "true")
        .output()
        .ok()?;
    let mut res = ProviderUsage { provider: "antigravity".into(), ..Default::default() };
    let text = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = match serde_json::from_str(text.trim()) {
        Ok(v) => v,
        Err(_) => {
            res.error = "No pude leer el uso de Antigravity".into();
            return Some(res);
        }
    };
    let data = if v["command"]["data"].is_object() { &v["command"]["data"] } else { &v["data"] };
    for g in data["groups"].as_array().into_iter().flatten() {
        let group = g["name"].as_str().or(g["title"].as_str()).unwrap_or("");
        for b in g["buckets"].as_array().into_iter().flatten() {
            if b["disabled"].as_bool().unwrap_or(false) {
                continue;
            }
            let Some(rem) = b["remaining_fraction"].as_f64() else { continue };
            let win = match b["window"].as_str().unwrap_or("") { "5h" => "5 h", "weekly" => "Semana", w => w };
            let label = if group.is_empty() { win.to_string() } else { format!("{group} · {win}") };
            res.windows.push(Window { label, used: (1.0 - rem) * 100.0, resets_at: epoch(&b["reset_time"]) });
        }
    }
    Some(res)
}

pub fn which(bin: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    dirs.push(home().join(".local/bin"));
    dirs.push(home().join(".npm-global/bin"));
    if cfg!(windows) {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            dirs.push(PathBuf::from(appdata).join("npm")); // npm global shims
        }
        let exts = ["", ".exe", ".cmd", ".bat", ".ps1"];
        return dirs.into_iter().flat_map(|d| exts.iter().map(move |e| d.join(format!("{bin}{e}")))).find(|p| p.is_file());
    }
    if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    dirs.into_iter().map(|d| d.join(bin)).find(|p| p.is_file())
}

pub fn all() -> Vec<ProviderUsage> {
    let mut v = Vec::new();
    let (a, b, c) = std::thread::scope(|s| {
        let a = s.spawn(claude);
        let b = s.spawn(codex);
        let c = s.spawn(antigravity);
        (a.join().ok().flatten(), b.join().ok().flatten(), c.join().ok().flatten())
    });
    v.extend(a);
    v.extend(b);
    v.extend(c);
    v
}
