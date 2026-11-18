<script lang="ts">
  // Right column: the agent's own todo list (Progreso), what it did recently (Actividad), changes summary.
  import { api, type Insight, type Changes } from "./api";
  import { CheckCircle2, Circle, Loader, FileText, Pencil, Terminal, Search, Globe, Bot, Dot, Sparkles, FolderTree, GitCompare } from "@lucide/svelte";
  import FileTree from "./FileTree.svelte";

  let { path, changes, active, onOpenFile, onOpenChange }: {
    path: string | null; changes: Changes | null; active: boolean;
    onOpenFile: (file: string) => void; onOpenChange: (file: string) => void;
  } = $props();
  let tab = $state<"agente" | "archivos" | "cambios">("agente");
  const changedSet = $derived(new Set((changes?.files ?? []).map((f) => f.path)));
  let insight = $state<Insight | null>(null);

  $effect(() => {
    const p = path;
    insight = null;
    if (!p) return;
    const load = () => api.insight(p).then((v) => (insight = v)).catch(() => {});
    load();
    const iv = setInterval(load, active ? 3000 : 12000); // faster while the agent works
    return () => clearInterval(iv);
  });

  const done = $derived(insight?.todos.filter((t) => t.status === "completed").length ?? 0);
  const total = $derived(insight?.todos.length ?? 0);
  const icons: Record<string, any> = { read: FileText, edit: Pencil, run: Terminal, search: Search, web: Globe, agent: Bot };
  function when(ts: string) {
    const t = Date.parse(ts);
    if (!t) return "";
    const s = Math.round((Date.now() - t) / 1000);
    return s < 60 ? "ahora" : s < 3600 ? `hace ${Math.round(s / 60)} min` : `hace ${Math.round(s / 3600)} h`;
  }
</script>

<aside class="insp">
  <div class="tabs card">
    <button class:on={tab === "agente"} onclick={() => (tab = "agente")} title="Progreso y actividad"><Sparkles size={14} />Agente</button>
    <button class:on={tab === "archivos"} onclick={() => (tab = "archivos")} title="Archivos de la tarea"><FolderTree size={14} />Archivos</button>
    <button class:on={tab === "cambios"} onclick={() => (tab = "cambios")} title="Archivos modificados"><GitCompare size={14} />Cambios
      {#if changes?.files.length}<i>{changes.files.length}</i>{/if}</button>
  </div>

  {#if tab === "archivos"}
    <section class="card grow">
      {#if path}<FileTree {path} changed={changedSet} onOpen={onOpenFile} />{:else}<p class="dim3 small">Selecciona una tarea</p>{/if}
    </section>
  {:else if tab === "cambios"}
    <section class="card grow scroll">
      <div class="ih"><span class="label">{changes?.files.length ?? 0} archivos</span>{#if changes}<span><span class="add">+{changes.added}</span> <span class="del">−{changes.removed}</span></span>{/if}</div>
      {#each changes?.files ?? [] as f (f.path)}
        <button class="frow click" onclick={() => onOpenChange(f.path)} title={f.path}>
          <span class="ms" data-s={f.status}>{f.status}</span>
          <span class="fp2"><span class="mono">{f.path.split("/").pop()}</span><small class="dim3">{f.path.includes("/") ? f.path.slice(0, f.path.lastIndexOf("/")) : ""}</small></span>
          {#if f.binary}<span class="dim3 tiny">binario</span>{:else}<span class="tiny"><span class="add">+{f.added}</span> <span class="del">−{f.removed}</span></span>{/if}
        </button>
      {/each}
      {#if !changes?.files.length}<p class="dim3 small">Sin cambios todavía</p>{/if}
    </section>
  {:else}
  <section class="card">
    <div class="ih"><span class="label">Progreso</span>{#if total}<span class="dim3 small">{done} / {total}</span>{/if}</div>
    {#if total}
      <div class="seg">{#each insight!.todos as t}<i class:ok={t.status === "completed"} class:now={t.status === "in_progress"}></i>{/each}</div>
      {#each insight!.todos as t}
        <div class="todo" class:now={t.status === "in_progress"} class:ok={t.status === "completed"}>
          {#if t.status === "completed"}<CheckCircle2 size={14} />{:else if t.status === "in_progress"}<Loader size={14} />{:else}<Circle size={14} />{/if}
          <span>{t.content}</span>
        </div>
      {/each}
    {:else}
      <p class="dim3 small">{path ? "Cuando el agente arme su lista de pasos, aparece aquí." : "Selecciona una tarea"}</p>
    {/if}
  </section>

  <section class="card">
    <div class="ih"><span class="label">Actividad</span>{#if active}<span class="live">● en vivo</span>{/if}</div>
    {#if insight?.activity.length}
      <div class="tl">
        {#each insight.activity.slice(0, 9) as a, i (i)}
          {@const Icon = icons[a.kind] ?? Dot}
          <div class="ev"><span class="ei" data-k={a.kind}><Icon size={12} /></span>
            <div><span class="et">{a.text}</span><small class="dim3">{when(a.ts)}</small></div></div>
        {/each}
      </div>
    {:else}
      <p class="dim3 small">Sin actividad registrada todavía.</p>
    {/if}
  </section>

  <section class="card">
    <div class="ih"><span class="label">Cambios</span>{#if changes}<span><span class="add">+{changes.added}</span> <span class="del">−{changes.removed}</span></span>{/if}</div>
    {#each (changes?.files ?? []).slice(0, 6) as f (f.path)}
      <button class="frow click" onclick={() => onOpenChange(f.path)}><span class="mono fp">{f.path.split("/").pop()}</span>
        {#if f.binary}<span class="bar bin"></span>{:else}<span class="bar"><b style="width:{(f.added / Math.max(1, f.added + f.removed)) * 100}%"></b></span>{/if}</button>
    {/each}
    {#if (changes?.files.length ?? 0) > 6}<p class="dim3 small">y {changes!.files.length - 6} más…</p>{/if}
    {#if !changes?.files.length}<p class="dim3 small">Sin cambios todavía</p>{/if}
  </section>
  {/if}
</aside>

<style>
  .insp { display: flex; flex-direction: column; gap: 8px; overflow-y: auto; min-height: 0; }
  section { padding: 13px 14px; }
  .ih { display: flex; justify-content: space-between; align-items: center; margin-bottom: 9px; }
  .small { font-size: 11.5px; }
  .seg { display: flex; gap: 3px; margin-bottom: 10px; }
  .seg i { flex: 1; height: 5px; border-radius: 3px; background: var(--s3); }
  .seg i.ok { background: var(--done); } .seg i.now { background: var(--working); }
  .todo { display: flex; gap: 8px; align-items: flex-start; font-size: 12px; padding: 3px 0; color: var(--text3); line-height: 1.35; }
  .todo :global(svg) { flex: none; margin-top: 1px; }
  .todo.ok { color: var(--text2); } .todo.ok :global(svg) { color: var(--done); }
  .todo.now { color: var(--text); font-weight: 700; } .todo.now :global(svg) { color: var(--working); }
  .live { font-size: 10.5px; font-weight: 800; color: var(--done); }
  .tl { display: flex; flex-direction: column; gap: 7px; }
  .ev { display: flex; gap: 9px; align-items: flex-start; }
  .ei { width: 22px; height: 22px; border-radius: 7px; background: var(--s3); display: flex; align-items: center; justify-content: center; flex: none; color: var(--text2); }
  .ei[data-k="edit"] { color: var(--done); } .ei[data-k="run"] { color: var(--waiting); } .ei[data-k="read"] { color: #64a8ff; }
  .et { display: block; font-size: 12px; line-height: 1.3; word-break: break-word; }
  .ev small { font-size: 10.5px; }
  .frow { width: 100%; display: flex; align-items: center; gap: 8px; padding: 3px 0; text-align: left; }
  .frow.click { border-radius: 7px; padding: 4px 6px; margin: 0 -6px; width: calc(100% + 12px); }
  .frow.click:hover { background: var(--s2); }
  .bar.bin { background: var(--s3); }
  .tabs { display: flex; padding: 4px; gap: 2px; }
  .tabs button { flex: 1; display: flex; align-items: center; justify-content: center; gap: 5px; padding: 6px 4px; border-radius: 9px; font-size: 12px; font-weight: 600; color: var(--text2); }
  .tabs button.on { background: var(--s3); color: var(--text); }
  .tabs i { font-style: normal; font-size: 10px; background: var(--s3); border-radius: 5px; padding: 0 5px; }
  .grow { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .scroll { overflow-y: auto; }
  .fp2 { flex: 1; min-width: 0; display: flex; flex-direction: column; font-size: 11.5px; }
  .fp2 span, .fp2 small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fp2 small { font-size: 10.5px; }
  .tiny { font-size: 11px; white-space: nowrap; }
  .ms { width: 18px; height: 18px; border-radius: 5px; font-size: 10px; font-weight: 800; display: flex; align-items: center; justify-content: center; background: #ff9f0a26; color: #ff9f0a; flex: none; }
  .ms[data-s="A"] { background: #30d15826; color: var(--done); } .ms[data-s="D"] { background: #ff453a26; color: var(--danger); }
  .fp { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11.5px; }
  .bar { width: 40px; height: 6px; border-radius: 3px; background: var(--danger); overflow: hidden; flex: none; }
  .bar b { display: block; height: 100%; background: var(--done); }
</style>
