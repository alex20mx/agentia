<script lang="ts">
  import { api, AGENTS, type Settings, type Project } from "./api";
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import { X, FolderPlus, Trash2, Download } from "@lucide/svelte";

  let { settings, projects, onClose, onSaved, onToast }: {
    settings: Settings; projects: Project[]; onClose: () => void; onSaved: () => void; onToast: (m: string) => void;
  } = $props();

  let s = $state<Settings>({ ...settings });

  async function save() {
    try {
      await api.saveSettings({ ...s, sleep_after_min: Number(s.sleep_after_min) || 0 });
      onToast("Ajustes guardados");
      onSaved();
      onClose();
    } catch (e) { onToast(String(e)); }
  }
  async function addProject() {
    const dir = await pickFolder({ directory: true, title: "Elige la carpeta del proyecto (repositorio git)" });
    if (!dir || Array.isArray(dir)) return;
    try { await api.addProject(dir); onToast("Proyecto agregado"); onSaved(); } catch (e) { onToast(String(e)); }
  }
  async function remove(p: Project) {
    if (!confirm(`¿Quitar ${p.name} de Agentia? (no se borra nada del disco)`)) return;
    await api.removeProject(p.id); onSaved();
  }
  async function importOrca() {
    try { const n = await api.importOrca(); onToast(n ? `${n} proyectos importados de Orca` : "No hay proyectos nuevos en Orca"); onSaved(); }
    catch (e) { onToast(String(e)); }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />
<div class="scrim" role="presentation" onclick={onClose}></div>
<div class="sheet card" role="dialog" aria-label="Ajustes">
  <header><b>Ajustes</b><button class="x" onclick={onClose} aria-label="Cerrar"><X size={16} /></button></header>
  <div class="body">
    <section>
      <span class="label">Proyectos</span>
      {#each projects as p (p.id)}
        <div class="proj"><span class="av">{p.name[0]}</span><div><b>{p.name}</b><small class="dim3 mono">{p.path}</small></div>
          <button class="ib" title="Quitar" onclick={() => remove(p)}><Trash2 size={14} /></button></div>
      {/each}
      <div class="row">
        <button class="btn" onclick={addProject}><FolderPlus size={14} />Agregar proyecto</button>
        <button class="btn" onclick={importOrca}><Download size={14} />Importar de Orca</button>
      </div>
    </section>
    <section class="grid">
      <label><span class="label">Carpeta de las tareas</span><input class="field mono" bind:value={s.workspace_dir} /></label>
      <label><span class="label">Prefijo de ramas</span><input class="field mono" bind:value={s.branch_prefix} /><small class="dim3">Las ramas se llaman {s.branch_prefix || "…"}/nombre-de-tarea</small></label>
      <label><span class="label">Dormir agentes tras (min)</span><input class="field" type="number" min="0" bind:value={s.sleep_after_min} /><small class="dim3">0 = nunca. Un agente listo y sin uso se duerme para liberar RAM.</small></label>
      <label><span class="label">Agente por defecto</span>
        <select class="field" bind:value={s.default_agent}>{#each Object.entries(AGENTS) as [id, a]}<option value={id}>{a.name}</option>{/each}</select></label>
      <label><span class="label">Editor</span><input class="field mono" bind:value={s.editor} placeholder="code, zed, cursor…" /><small class="dim3">Comando para "Abrir en editor"</small></label>
    </section>
    <section class="about">
      <img src="/agentia-icon.svg" alt="" width="40" height="40" />
      <div><b>Agentia</b><small class="dim3">Sin API keys: usa las sesiones que ya tienes en Claude, Codex, Copilot y OpenCode. Nada sale de tu compu salvo hacia las APIs oficiales para ver tu uso.</small></div>
    </section>
  </div>
  <footer><button class="btn" onclick={onClose}>Cancelar</button><button class="btn blue" onclick={save}>Guardar</button></footer>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: #0009; z-index: 20; }
  .sheet { position: fixed; z-index: 21; left: 50%; top: 50%; transform: translate(-50%, -50%); width: min(680px, 94vw); max-height: 88vh; display: flex; flex-direction: column; box-shadow: 0 30px 80px #000c; }
  header { display: flex; align-items: center; padding: 16px 18px; border-bottom: 1px solid var(--line); }
  header b { font-family: var(--display); font-size: 18px; }
  .x { margin-left: auto; width: 30px; height: 30px; border-radius: 9px; display: flex; align-items: center; justify-content: center; }
  .x:hover { background: var(--s3); }
  .body { overflow-y: auto; padding: 8px 18px 16px; display: flex; flex-direction: column; gap: 18px; }
  section { display: flex; flex-direction: column; gap: 8px; }
  .proj { display: flex; align-items: center; gap: 10px; padding: 7px 8px; border-radius: 10px; background: var(--s2); }
  .proj div { flex: 1; min-width: 0; } .proj small { display: block; font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .av { width: 24px; height: 24px; border-radius: 7px; background: #0a84ff26; color: #64a8ff; font-weight: 800; display: flex; align-items: center; justify-content: center; }
  .ib { width: 28px; height: 28px; border-radius: 8px; display: flex; align-items: center; justify-content: center; color: var(--text3); }
  .ib:hover { background: var(--s3); color: var(--danger); }
  .row { display: flex; gap: 8px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  label { display: flex; flex-direction: column; gap: 5px; }
  label small { font-size: 11px; }
  .field { background: var(--s2); border: 1px solid var(--line2); border-radius: 9px; padding: 8px 10px; outline: none; width: 100%; user-select: text; }
  .about { flex-direction: row; align-items: center; gap: 12px; padding: 12px; border-radius: 12px; background: var(--s2); }
  .about small { display: block; font-size: 11.5px; line-height: 1.4; margin-top: 2px; }
  footer { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 18px; border-top: 1px solid var(--line); }
</style>
