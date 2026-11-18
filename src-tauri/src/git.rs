//! Git helpers: worktrees (one per task), base branch detection and change stats.
//! Everything shells out to `git`, the same commands Orca uses.
use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::path::Path;
use std::process::Command;

pub fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .with_context(|| format!("no se pudo ejecutar git {}", args.join(" ")))?;
    if !out.status.success() {
        return Err(anyhow!(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[derive(Serialize, Clone, Debug)]
pub struct Worktree {
    pub path: String,
    pub branch: String, // short name, "" when detached
    pub head: String,
    pub is_main: bool,
}

/// `git worktree list --porcelain` → the repo's main checkout first, then every task worktree.
pub fn list_worktrees(repo: &Path) -> Result<Vec<Worktree>> {
    let text = git(repo, &["worktree", "list", "--porcelain"])?;
    let mut out = Vec::new();
    for block in text.split("\n\n") {
        let mut wt = Worktree { path: String::new(), branch: String::new(), head: String::new(), is_main: out.is_empty() };
        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                wt.path = p.to_string();
            } else if let Some(h) = line.strip_prefix("HEAD ") {
                wt.head = h.chars().take(8).collect();
            } else if let Some(b) = line.strip_prefix("branch ") {
                wt.branch = b.trim_start_matches("refs/heads/").to_string();
            }
        }
        if !wt.path.is_empty() && Path::new(&wt.path).exists() {
            out.push(wt);
        }
    }
    Ok(out)
}

/// Default base like Orca: origin/HEAD, else origin/main|master, else main|master.
pub fn default_base(repo: &Path) -> String {
    if let Ok(r) = git(repo, &["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"]) {
        let r = r.trim();
        if !r.is_empty() {
            return r.to_string();
        }
    }
    for cand in ["origin/main", "origin/master", "main", "master"] {
        if git(repo, &["rev-parse", "--verify", "--quiet", cand]).is_ok() {
            return cand.to_string();
        }
    }
    "HEAD".to_string()
}

/// Lowercase, accents removed, spaces → dashes: "Arreglar QR de WhatsApp" → "arreglar-qr-de-whatsapp".
pub fn slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.trim().to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ñ' => 'n',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            s.push(c);
        } else if !s.ends_with('-') {
            s.push('-');
        }
    }
    let s = s.trim_matches('-').chars().take(48).collect::<String>();
    if s.is_empty() { "tarea".into() } else { s }
}

/// Create the task's worktree: `git worktree add --no-track -b <branch> <path> <base>`,
/// then remember the base (`branch.<b>.base`) so diffs know what to compare against.
pub fn create_worktree(repo: &Path, path: &Path, branch: &str, base: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let p = path.to_string_lossy();
    git(repo, &["worktree", "add", "--no-track", "-b", branch, &p, base])?;
    let _ = git(repo, &["config", "--local", &format!("branch.{branch}.base"), base]);
    Ok(())
}

/// Copy gitignored files listed in `.worktreeinclude` (e.g. `.env`) from the main checkout.
pub fn copy_includes(repo: &Path, worktree: &Path) -> Vec<String> {
    let mut copied = Vec::new();
    let list = std::fs::read_to_string(repo.join(".worktreeinclude")).unwrap_or_default();
    let mut names: Vec<String> = list
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect();
    if names.is_empty() {
        names = vec![".env".into(), ".env.local".into()]; // sensible default
    }
    for n in names {
        if n.contains("..") {
            continue; // never copy outside the repo
        }
        let src = repo.join(&n);
        if src.is_file() && !worktree.join(&n).exists() {
            if let Some(parent) = worktree.join(&n).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::copy(&src, worktree.join(&n)).is_ok() {
                copied.push(n);
            }
        }
    }
    copied
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct FileChange {
    pub path: String,
    pub status: String, // M A D R ?
    pub added: u32,
    pub removed: u32,
    pub binary: bool, // images etc.: no line counts
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Changes {
    pub base: String,
    pub files: Vec<FileChange>,
    pub added: u32,
    pub removed: u32,
}

/// Everything the task changed vs. its base: committed (merge-base..HEAD) + uncommitted + untracked.
pub fn changes(worktree: &Path) -> Result<Changes> {
    let branch = git(worktree, &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string();
    let base = git(worktree, &["config", "--get", &format!("branch.{branch}.base")])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| default_base(worktree));
    let base = base.trim_start_matches("refs/heads/").to_string();
    let mb = git(worktree, &["merge-base", &base, "HEAD"]).map(|s| s.trim().to_string()).unwrap_or_else(|_| "HEAD".into());
    let mut files: Vec<FileChange> = Vec::new();
    for line in git(worktree, &["diff", "--numstat", &mb])?.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() == 3 {
            files.push(FileChange {
                path: parts[2].to_string(),
                status: "M".into(),
                added: parts[0].parse().unwrap_or(0),
                removed: parts[1].parse().unwrap_or(0),
                binary: parts[0] == "-",
            });
        }
    }
    for line in git(worktree, &["diff", "--name-status", &mb])?.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            if let Some(f) = files.iter_mut().find(|f| &f.path == parts.last().unwrap()) {
                f.status = parts[0].chars().next().unwrap_or('M').to_string();
            }
        }
    }
    for line in git(worktree, &["ls-files", "--others", "--exclude-standard"])?.lines() {
        let text = std::fs::read_to_string(worktree.join(line));
        let binary = text.is_err();
        let lines = text.map(|t| t.lines().count() as u32).unwrap_or(0);
        files.push(FileChange { path: line.to_string(), status: "A".into(), added: lines, removed: 0, binary });
    }
    let added = files.iter().map(|f| f.added).sum();
    let removed = files.iter().map(|f| f.removed).sum();
    Ok(Changes { base, files, added, removed })
}

fn merge_base(worktree: &Path) -> String {
    let branch = git(worktree, &["rev-parse", "--abbrev-ref", "HEAD"]).map(|s| s.trim().to_string()).unwrap_or_default();
    let base = git(worktree, &["config", "--get", &format!("branch.{branch}.base")])
        .map(|s| s.trim().trim_start_matches("refs/heads/").to_string())
        .unwrap_or_else(|_| default_base(worktree));
    git(worktree, &["merge-base", &base, "HEAD"]).map(|s| s.trim().to_string()).unwrap_or_else(|_| "HEAD".into())
}

/// Unified diff of one file vs. the task's base (committed + uncommitted). Untracked files show as all-added.
pub fn file_diff(worktree: &Path, file: &str) -> Result<String> {
    if file.contains("..") {
        return Err(anyhow!("ruta inválida"));
    }
    let tracked = git(worktree, &["ls-files", "--error-unmatch", "--", file]).is_ok()
        || !git(worktree, &["log", "-1", "--format=%H", "--", file]).unwrap_or_default().trim().is_empty();
    if !tracked {
        let text = std::fs::read(worktree.join(file)).map_err(|e| anyhow!(e))?;
        if text.iter().take(8000).any(|b| *b == 0) {
            return Ok("Archivo binario nuevo".into());
        }
        let text = String::from_utf8_lossy(&text);
        let lines: Vec<String> = text.lines().take(3000).map(|l| format!("+{l}")).collect();
        return Ok(format!("@@ archivo nuevo @@\n{}", lines.join("\n")));
    }
    let mb = merge_base(worktree);
    git(worktree, &["diff", "--no-color", "-U3", &mb, "--", file])
}

/// Stage everything and commit.
pub fn commit_all(worktree: &Path, message: &str) -> Result<String> {
    if message.trim().is_empty() {
        return Err(anyhow!("Escribe un mensaje para el commit"));
    }
    git(worktree, &["add", "-A"])?;
    git(worktree, &["commit", "-m", message])?;
    Ok(git(worktree, &["log", "-1", "--format=%h"])?.trim().to_string())
}

/// Throw away uncommitted changes of one file (tracked → restore, untracked → delete).
pub fn discard_file(worktree: &Path, file: &str) -> Result<()> {
    if file.contains("..") {
        return Err(anyhow!("ruta inválida"));
    }
    if git(worktree, &["ls-files", "--error-unmatch", "--", file]).is_ok() {
        git(worktree, &["restore", "--staged", "--worktree", "--", file])?;
    } else {
        std::fs::remove_file(worktree.join(file)).map_err(|e| anyhow!(e))?;
    }
    Ok(())
}

pub fn uncommitted(worktree: &Path) -> usize {
    git(worktree, &["status", "--porcelain"]).map(|s| s.lines().count()).unwrap_or(0)
}

pub fn push(worktree: &Path) -> Result<String> {
    git(worktree, &["push", "--set-upstream", "origin", "HEAD"])
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct PrInfo {
    pub number: u64,
    pub url: String,
    pub state: String,
    pub title: String,
}

fn gh(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("gh").current_dir(dir).args(args).output().map_err(|_| anyhow!("Instala GitHub CLI (gh) e inicia sesión con `gh auth login` para crear PRs"))?;
    if !out.status.success() {
        return Err(anyhow!(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// PR for the current branch, if any.
pub fn pr_info(worktree: &Path) -> Option<PrInfo> {
    let out = gh(worktree, &["pr", "view", "--json", "number,url,state,title"]).ok()?;
    serde_json::from_str::<serde_json::Value>(&out).ok().map(|v| PrInfo {
        number: v["number"].as_u64().unwrap_or(0),
        url: v["url"].as_str().unwrap_or_default().into(),
        state: v["state"].as_str().unwrap_or_default().into(),
        title: v["title"].as_str().unwrap_or_default().into(),
    })
}

/// Push and open a PR against the task's base branch.
pub fn create_pr(worktree: &Path, title: &str, body: &str, draft: bool) -> Result<PrInfo> {
    push(worktree)?;
    let branch = git(worktree, &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string();
    let base = git(worktree, &["config", "--get", &format!("branch.{branch}.base")])
        .map(|s| s.trim().trim_start_matches("refs/heads/").trim_start_matches("origin/").to_string())
        .unwrap_or_else(|_| default_base(worktree).trim_start_matches("origin/").to_string());
    let mut args = vec!["pr", "create", "--base", &base, "--title", title, "--body", body];
    if draft {
        args.push("--draft");
    }
    gh(worktree, &args)?;
    pr_info(worktree).ok_or_else(|| anyhow!("El PR se creó pero no pude leerlo"))
}

/// Every file of the task (tracked + new, respecting .gitignore), for the Files tree.
pub fn list_files(worktree: &Path) -> Result<Vec<String>> {
    let out = git(worktree, &["ls-files", "--cached", "--others", "--exclude-standard"])?;
    let mut v: Vec<String> = out.lines().take(20000).map(String::from).collect();
    v.sort();
    v.dedup();
    Ok(v)
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct FileContent {
    pub text: String,
    pub binary: bool,
    pub size: u64,
    pub truncated: bool,
}

/// Read a file of the task for the viewer (text only, first 1 MB).
pub fn read_file(worktree: &Path, file: &str) -> Result<FileContent> {
    if file.contains("..") || Path::new(file).is_absolute() {
        return Err(anyhow!("ruta inválida"));
    }
    let p = worktree.join(file);
    let size = std::fs::metadata(&p)?.len();
    let mut bytes = std::fs::read(&p)?;
    let truncated = bytes.len() > 1_000_000;
    bytes.truncate(1_000_000);
    if bytes.iter().take(8000).any(|b| *b == 0) {
        return Ok(FileContent { binary: true, size, ..Default::default() });
    }
    Ok(FileContent { text: String::from_utf8_lossy(&bytes).to_string(), binary: false, size, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("agentia-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for a in [vec!["init", "-q", "-b", "main"], vec!["config", "user.email", "t@t"], vec!["config", "user.name", "t"]] {
            git(&dir, &a).unwrap();
        }
        std::fs::write(dir.join("a.txt"), "uno\ndos\n").unwrap();
        std::fs::write(dir.join(".env"), "SECRET=1\n").unwrap();
        std::fs::write(dir.join(".gitignore"), ".env\n").unwrap();
        git(&dir, &["add", "-A"]).unwrap();
        git(&dir, &["commit", "-qm", "init"]).unwrap();
        dir
    }

    #[test]
    fn full_task_cycle() {
        let r = repo();
        assert_eq!(slug("Arreglar QR de WhatsApp ñ!"), "arreglar-qr-de-whatsapp-n");
        assert_eq!(default_base(&r), "main");
        let wt = r.parent().unwrap().join(format!("{}-wt", r.file_name().unwrap().to_string_lossy()));
        let _ = std::fs::remove_dir_all(&wt);
        create_worktree(&r, &wt, "yo/prueba", "main").unwrap();
        let wts = list_worktrees(&r).unwrap();
        assert_eq!(wts.len(), 2);
        assert!(wts[0].is_main && wts[1].branch == "yo/prueba");
        assert_eq!(copy_includes(&r, &wt), vec![".env".to_string()]);
        // edit + new file → changes and diffs
        std::fs::write(wt.join("a.txt"), "uno\nDOS\ntres\n").unwrap();
        std::fs::write(wt.join("b.txt"), "nuevo\n").unwrap();
        let c = changes(&wt).unwrap();
        assert_eq!(c.base, "main");
        assert_eq!(c.files.len(), 2, "{:?}", c.files.iter().map(|f| &f.path).collect::<Vec<_>>());
        assert!(file_diff(&wt, "a.txt").unwrap().contains("+DOS"));
        assert!(file_diff(&wt, "b.txt").unwrap().contains("+nuevo"));
        // discard the new file, commit the edit
        discard_file(&wt, "b.txt").unwrap();
        assert_eq!(uncommitted(&wt), 1);
        let h = commit_all(&wt, "cambio").unwrap();
        assert!(!h.is_empty());
        assert_eq!(uncommitted(&wt), 0);
        let c = changes(&wt).unwrap();
        assert_eq!(c.files.len(), 1); // committed change still shows vs. base
        assert!(file_diff(&wt, "../etc/passwd").is_err());
        // remove the task
        git(&r, &["worktree", "remove", "--force", "--", &wt.to_string_lossy()]).unwrap();
        git(&r, &["branch", "-D", "yo/prueba"]).unwrap();
        assert_eq!(list_worktrees(&r).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&r);
    }
}
