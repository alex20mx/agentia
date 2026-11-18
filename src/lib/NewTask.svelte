<script lang="ts">
  import { api, AGENTS, type Project } from "./api";
  import AgentLogo from "./AgentLogo.svelte";
  import { X, GitBranch, FolderOpen, Sparkles } from "@lucide/svelte";

  let { projects, initialProject, defaultAgent, onClose, onCreated }: {
    projects: Project[]; initialProject: string; defaultAgent: string;
    onClose: () => void; onCreated: (path: string) => void;
  } = $props();

  let projectId = $state(initialProject || projects[0]?.id || "");
  let title = $state("");
  let agent = $state(defaultAgent || "claude");
  let model = $state("");
  let prompt = $state("");
  let base = $state("");
  let copyEnv = $state(true);
  let busy = $state(false);
  let error = $state("");
  let preview = $state<{ path: string; branch: string } | null>(null);

  $effect(() => {
    const p = projectId;
    if (p) api.defaultBase(p).then((b) => (base = b)).catch(() => (base = ""));
  });
  $effect(() => {
    const p = projectId, t = title;
    if (p && t.trim()) api.preview(p, t).then((v) => (preview = v)).catch(() => (preview = null));
    else preview = null;
  });

  async function create(start: boolean) {
    if (!title.trim()) { error = "Ponle nombre a la tarea"; return; }
    busy = true; error = "";
    try {
      const r = await api.createTask({ project_id: projectId, title, agent, model, prompt, base, copy_env: copyEnv, start });
      onCreated(r.path);
    } catch (e) { error = String(e); } finally { busy = false; }
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
    if (e.key === "Enter" && e.ctrlKey) create(true);
  }
</script>

<svelte:window onkeydown={key} />
<div class="scrim" role="presentation" onclick={onClose}></div>
<div class="sheet card" role="dialog" aria-label="Nueva tarea">
  <header>
    <div class="ic"><Sparkles size={18} /></div>
    <div><b>Nueva tarea</b><small class="dim">Crea su propia rama y arranca un agente en ella</small></div>
    <button class="x" onclick={onClose} aria-label="Cerrar"><X size={16} /></button>
  </header>
  <div class="body">
    <div class="form">
      <span class="label">Proyecto</span>
      <div class="projects">
        {#each projects as p (p.id)}
          <button class="proj" class:on={p.id === projectId} onclick={() => (projectId = p.id)}>
            <span class="av">{p.name[0]}</span><span>{p.name}</span>
          </button>
        {/each}
      </div>

      <span class="label">Nombre de la tarea</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="field" bind:value={title} placeholder="Ej. Exportar reportes a PDF" autofocus />

      <span class="label">Agente</span>
      <div class="agents">
        {#each Object.entries(AGENTS) as [id, a] (id)}
          <button class="agent" class:on={agent === id} style="--c:{a.color}" onclick={() => { agent = id; model = ""; }}>
            <AgentLogo {id} size={18} /><span>{a.name}</span>
          </button>
        {/each}
      </div>
      {#if AGENTS[agent].models.length > 1}
        <div class="models">
          {#each AGENTS[agent].models as m}
            <button class="chip" class:on={model === m} onclick={() => (model = m)}>{m || "predeterminado"}</button>
          {/each}
        </div>
      {/if}

      <span class="label">Instrucciones</span>
      <textarea class="field area" bind:value={prompt} placeholder="Qué quieres que haga el agente… (puedes dejarlo vacío y escribirle después)"></textarea>

      <div class="row2">
        <div>
          <span class="label">Rama base</span>
          <input class="field mono" bind:value={base} />
        </div>
        <label class="opt">
          <input type="checkbox" bind:checked={copyEnv} />
          <span><b>Copiar .env</b><small class="dim3">desde la carpeta principal del proyecto</small></span>
        </label>
      </div>
      {#if error}<p class="err">{error}</p>{/if}
    </div>

    <aside class="preview">
      <span class="label">Se va a crear</span>
      <div class="pv"><FolderOpen size={14} /><span class="mono">{preview?.path ?? "—"}</span></div>
      <div class="pv"><GitBranch size={14} /><span class="mono">{preview?.branch ?? "—"} ← {base || "main"}</span></div>
      <span class="label" style="margin-top:14px">Comando</span>
      <pre class="cmd">$ git worktree add …
$ {agent}{model ? ` --model ${model}` : ""} {prompt ? `"${prompt.slice(0, 40)}${prompt.length > 40 ? "…" : ""}"` : ""}</pre>
      <p class="dim3 note">Tu rama actual no se toca: el agente trabaja en su propia carpeta. Si lo dejas sin trabajar, se duerme solo para ahorrar RAM.</p>
    </aside>
  </div>
  <footer>
    <span class="dim3">Ctrl + Enter para crear</span>
    <button class="btn" onclick={onClose}>Cancelar</button>
    <button class="btn" disabled={busy} onclick={() => create(false)}>Guardar en Por hacer</button>
    <button class="btn blue" disabled={busy} onclick={() => create(true)}>{busy ? "Creando…" : "Crear y arrancar"}</button>
  </footer>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: #000a; z-index: 20; }
  .sheet { position: fixed; z-index: 21; left: 50%; top: 50%; transform: translate(-50%, -50%); width: min(920px, 94vw);
    max-height: 90vh; display: flex; flex-direction: column; box-shadow: 0 30px 80px #000c; }
  header { display: flex; gap: 12px; align-items: center; padding: 16px 18px; border-bottom: 1px solid var(--line); }
  header b { font-family: var(--display); font-size: 17px; display: block; }
  header small { font-size: 12px; }
  .ic { width: 36px; height: 36px; border-radius: 11px; background: linear-gradient(135deg, var(--working), #5e5ce6);
    display: flex; align-items: center; justify-content: center; }
  .x { margin-left: auto; width: 30px; height: 30px; border-radius: 9px; display: flex; align-items: center; justify-content: center; }
  .x:hover { background: var(--s3); }
  .body { display: grid; grid-template-columns: 1fr 300px; overflow: auto; }
  .form { padding: 16px 18px; display: flex; flex-direction: column; gap: 8px; }
  .label { margin-top: 6px; }
  .projects, .agents, .models { display: flex; flex-wrap: wrap; gap: 6px; }
  .proj, .agent { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-radius: var(--r3); background: var(--s2);
    border: 1px solid var(--line2); font-weight: 600; }
  .proj.on { border-color: var(--working); box-shadow: 0 0 0 1px var(--working) inset; }
  .av { width: 20px; height: 20px; border-radius: 6px; background: #0a84ff30; color: #64a8ff; font-size: 11px; font-weight: 800;
    display: flex; align-items: center; justify-content: center; }
  .agent.on { border-color: var(--c); box-shadow: 0 0 0 1px var(--c) inset; }
  .chip { font-size: 11.5px; font-weight: 700; padding: 4px 10px; border-radius: 8px; background: var(--s2); border: 1px solid var(--line2); }
  .chip.on { background: var(--text); color: #000; }
  .field { background: var(--s2); border: 1px solid var(--line2); border-radius: var(--r3); padding: 9px 12px; outline: none; width: 100%; }
  .field:focus { border-color: var(--working); }
  .area { min-height: 110px; resize: vertical; user-select: text; }
  .row2 { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; align-items: end; }
  .opt { display: flex; gap: 10px; align-items: center; padding: 8px 0; }
  .opt small { display: block; font-size: 11px; }
  .err { color: var(--danger); font-weight: 600; }
  .preview { border-left: 1px solid var(--line); padding: 16px; background: #0b0b0e; display: flex; flex-direction: column; gap: 8px; }
  .pv { display: flex; gap: 8px; align-items: flex-start; font-size: 11.5px; color: var(--text2); word-break: break-all; }
  .cmd { font-family: var(--mono); font-size: 11.5px; background: var(--s1); border: 1px solid var(--line); border-radius: var(--r2);
    padding: 10px; white-space: pre-wrap; color: var(--text2); }
  .note { font-size: 11.5px; line-height: 1.45; margin-top: auto; }
  footer { display: flex; gap: 8px; align-items: center; padding: 12px 18px; border-top: 1px solid var(--line); }
  footer span { margin-right: auto; font-size: 11.5px; }
</style>
