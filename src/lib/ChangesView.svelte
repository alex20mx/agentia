<script lang="ts">
  // Review a task: files changed vs. its base, the diff of each, commit / discard / push / PR.
  import { api, type Changes, type PrInfo } from "./api";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { GitCommitHorizontal, Undo2, UploadCloud, GitPullRequest, ExternalLink } from "@lucide/svelte";

  let { path, changes, focusFile = null, onChanged, onToast }: {
    path: string; changes: Changes | null; focusFile?: string | null; onChanged: () => void; onToast: (m: string) => void;
  } = $props();

  let file = $state<string | null>(null);
  let diff = $state("");
  let message = $state("");
  let busy = $state("");
  let dirty = $state(0);
  let pr = $state<PrInfo | null>(null);
  let showPr = $state(false);
  let prTitle = $state("");
  let prBody = $state("");
  let prDraft = $state(false);

  $effect(() => {
    const p = path;
    file = null; diff = ""; pr = null;
    api.prInfo(p).then((v) => (pr = v)).catch(() => {});
  });
  $effect(() => { if (focusFile) file = focusFile; });
  $effect(() => {
    const files = changes?.files ?? [];
    if (!file && files.length) file = files[0].path;
    api.uncommitted(path).then((n) => (dirty = n)).catch(() => {});
  });
  $effect(() => {
    const f = file, p = path;
    if (!f) { diff = ""; return; }
    api.fileDiff(p, f).then((d) => (diff = d)).catch((e) => (diff = String(e)));
  });

  const lines = $derived(diff.split("\n").slice(0, 4000));

  async function run(label: string, fn: () => Promise<unknown>, ok: string) {
    busy = label;
    try { await fn(); onToast(ok); onChanged(); } catch (e) { onToast(String(e)); } finally { busy = ""; }
  }
  const commit = () => run("commit", async () => { await api.commit(path, message); message = ""; }, "Commit hecho ✓");
  const push = () => run("push", () => api.push(path), "Cambios subidos ✓");
  const discard = (f: string) => {
    if (!confirm(`¿Descartar los cambios sin guardar de ${f}?`)) return;
    run("discard", () => api.discard(path, f), "Cambios descartados");
  };
  async function createPr() {
    busy = "pr";
    try { pr = await api.createPr(path, prTitle, prBody, prDraft); showPr = false; onToast(`PR #${pr.number} creado ✓`); }
    catch (e) { onToast(String(e)); } finally { busy = ""; }
  }
  export function openPr(title: string) { prTitle = title; showPr = true; }
</script>

<div class="wrap">
  <aside class="files">
    <div class="fh"><span class="label">{changes?.files.length ?? 0} archivos</span>
      {#if changes}<span><span class="add">+{changes.added}</span> <span class="del">−{changes.removed}</span></span>{/if}</div>
    <div class="flist">
      {#each changes?.files ?? [] as f (f.path)}
        <button class="frow" class:on={file === f.path} onclick={() => (file = f.path)}>
          <span class="ms" data-s={f.status}>{f.status}</span>
          <span class="fp mono" title={f.path}>{f.path.split("/").pop()}<small>{f.path.includes("/") ? f.path.slice(0, f.path.lastIndexOf("/")) : ""}</small></span>
          {#if f.binary}<span class="n dim3">bin</span>{:else}<span class="n"><span class="add">+{f.added}</span><span class="del">−{f.removed}</span></span>{/if}
        </button>
      {/each}
      {#if !changes?.files.length}<p class="dim3 empty">Sin cambios todavía</p>{/if}
    </div>
    <div class="commit">
      <textarea bind:value={message} rows="2" placeholder={dirty ? `Mensaje del commit (${dirty} sin guardar)` : "Nada pendiente de commit"}></textarea>
      <div class="cbtns">
        <button class="btn" disabled={!dirty || !message.trim() || !!busy} onclick={commit}><GitCommitHorizontal size={14} />{busy === "commit" ? "…" : "Commit"}</button>
        <button class="btn" disabled={!!busy} onclick={push}><UploadCloud size={14} />{busy === "push" ? "Subiendo…" : "Push"}</button>
      </div>
      {#if pr}
        <button class="prlink" onclick={() => openUrl(pr!.url)}><GitPullRequest size={14} />PR #{pr.number} · {pr.state.toLowerCase()}<ExternalLink size={12} /></button>
      {:else}
        <button class="btn primary wide" disabled={!!busy} onclick={() => (showPr = true)}><GitPullRequest size={14} />Crear PR</button>
      {/if}
    </div>
  </aside>
  <section class="diff mono">
    {#if file}
      <div class="dh"><span>{file}</span>
        <button class="btn" onclick={() => discard(file!)} disabled={!!busy}><Undo2 size={13} />Descartar</button></div>
      <div class="dl">
        {#each lines as l, i (i)}
          <div class="ln" class:a={l.startsWith("+") && !l.startsWith("+++")} class:d={l.startsWith("-") && !l.startsWith("---")} class:h={l.startsWith("@@")}>{l || " "}</div>
        {/each}
      </div>
    {:else}
      <p class="dim empty">Elige un archivo para ver qué cambió</p>
    {/if}
  </section>
</div>

{#if showPr}
  <div class="scrim" role="presentation" onclick={() => (showPr = false)}></div>
  <div class="prsheet card" role="dialog">
    <b class="pt">Crear Pull Request</b>
    <span class="label">Título</span>
    <input class="field" bind:value={prTitle} />
    <span class="label">Descripción</span>
    <textarea class="field" rows="6" bind:value={prBody} placeholder="Qué cambia y por qué…"></textarea>
    <label class="draft"><input type="checkbox" bind:checked={prDraft} /> Como borrador</label>
    <p class="dim3 small">Se suben tus commits y se abre el PR con GitHub CLI (gh). Lo que no tenga commit no se incluye.</p>
    <div class="pb"><button class="btn" onclick={() => (showPr = false)}>Cancelar</button>
      <button class="btn blue" disabled={!prTitle.trim() || busy === "pr"} onclick={createPr}>{busy === "pr" ? "Creando…" : "Crear PR"}</button></div>
  </div>
{/if}

<style>
  .wrap { flex: 1; min-height: 0; display: grid; grid-template-columns: 300px 1fr; margin: 8px 10px 10px; gap: 8px; }
  .files { display: flex; flex-direction: column; min-height: 0; border: 1px solid var(--line); border-radius: 12px; background: var(--s1); }
  .fh { display: flex; justify-content: space-between; padding: 10px 12px; border-bottom: 1px solid var(--line); }
  .flist { flex: 1; overflow-y: auto; padding: 4px; }
  .frow { width: 100%; display: flex; align-items: center; gap: 8px; padding: 6px 8px; border-radius: 8px; text-align: left; }
  .frow:hover { background: var(--s2); } .frow.on { background: var(--s3); }
  .fp { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
  .fp small { display: block; color: var(--text3); font-size: 10.5px; overflow: hidden; text-overflow: ellipsis; }
  .n { display: flex; gap: 5px; font-size: 11px; }
  .ms { width: 18px; height: 18px; border-radius: 5px; font-size: 10px; font-weight: 800; display: flex; align-items: center; justify-content: center; background: #ff9f0a26; color: #ff9f0a; flex: none; font-family: var(--font); }
  .ms[data-s="A"] { background: #30d15826; color: var(--done); } .ms[data-s="D"] { background: #ff453a26; color: var(--danger); }
  .commit { border-top: 1px solid var(--line); padding: 10px; display: flex; flex-direction: column; gap: 8px; }
  .commit textarea, .field { background: var(--s2); border: 1px solid var(--line2); border-radius: 9px; padding: 8px 10px; outline: none; resize: none; width: 100%; user-select: text; }
  .cbtns { display: flex; gap: 6px; } .cbtns .btn { flex: 1; justify-content: center; }
  .wide { justify-content: center; width: 100%; }
  .prlink { display: flex; align-items: center; gap: 6px; justify-content: center; padding: 7px; border-radius: 9px; background: #30d15822; color: var(--done); font-weight: 700; font-size: 12.5px; }
  .diff { min-height: 0; display: flex; flex-direction: column; border: 1px solid var(--line); border-radius: 12px; background: var(--term); overflow: hidden; }
  .dh { display: flex; justify-content: space-between; align-items: center; padding: 8px 12px; border-bottom: 1px solid var(--line); font-size: 12px; color: var(--text2); font-family: var(--mono); }
  .dl { flex: 1; overflow: auto; padding: 6px 0; font-size: 12px; line-height: 1.55; user-select: text; }
  .ln { padding: 0 14px; white-space: pre; color: #c7c7cc; }
  .ln.a { background: #30d15818; color: #7ee39a; } .ln.d { background: #ff453a18; color: #ff8a82; }
  .ln.h { color: #64a8ff; background: #0a84ff12; margin-top: 4px; }
  .empty { padding: 30px; text-align: center; }
  .scrim { position: fixed; inset: 0; background: #0009; z-index: 20; }
  .prsheet { position: fixed; z-index: 21; left: 50%; top: 50%; transform: translate(-50%, -50%); width: 520px; padding: 18px; display: flex; flex-direction: column; gap: 8px; box-shadow: 0 30px 80px #000c; }
  .pt { font-family: var(--display); font-size: 17px; }
  .draft { display: flex; gap: 8px; align-items: center; font-weight: 600; }
  .pb { display: flex; justify-content: flex-end; gap: 8px; margin-top: 6px; }
  .small { font-size: 11.5px; }
</style>
