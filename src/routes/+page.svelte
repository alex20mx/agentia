<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
  import { api, AGENTS, STATUS, ago, type Snapshot, type Task, type Project, type Changes, type Status } from "$lib/api";
  import Terminal from "$lib/Terminal.svelte";
  import NewTask from "$lib/NewTask.svelte";
  import ChangesView from "$lib/ChangesView.svelte";
  import Inspector from "$lib/Inspector.svelte";
  import Board from "$lib/Board.svelte";
  import SettingsSheet from "$lib/SettingsSheet.svelte";
  import UsageBar from "$lib/UsageBar.svelte";
  import FileView from "$lib/FileView.svelte";
  import AgentLogo from "$lib/AgentLogo.svelte";
  import {
    ListTodo, KanbanSquare, Settings2, Search, Plus, ChevronDown, ChevronRight, GitBranch, Folder, Palette,
    Moon, Sun, Square, GitPullRequest, Code2, Terminal as TermIcon, FileDiff, Bell,
    Play, X, Trash2, FileText, MoreHorizontal, Pencil, Cpu, SlidersHorizontal, FolderPlus, Folder as FolderIcon, CheckCircle2,
  } from "@lucide/svelte";

  // ── state
  let snap = $state<Snapshot | null>(null);
  let selected = $state<string | null>(null);
  let view = $state<"tareas" | "tablero">("tareas");
  let filter = $state<"todas" | "activas" | "necesitan" | "listas">("todas");
  let query = $state("");
  let showNew = $state(false);
  let showSettings = $state(false);
  let tab = $state("agente"); // "agente" | "cambios" | "archivo" | extra terminal id
  let openFile = $state<string | null>(null);
  let focusChange = $state<string | null>(null);
  let changes = $state<Changes | null>(null);
  let toast = $state("");
  let menu = $state(false);
  let renaming = $state(false);
  let newTitle = $state("");
  let terms = $state<Record<string, { id: string; label: string; command: string }[]>>({}); // extra terminals per task
  let scripts = $state<string[]>([]);
  let showRun = $state(false);
  let showFilters = $state(false);
  let termRef = $state<{ reload: () => void; focus: () => void } | null>(null);
  let changesRef = $state<{ openPr: (t: string) => void } | null>(null);

  // theme
  const THEMES = [["grafito", "Grafito"], ["medianoche", "Medianoche"], ["negro", "Negro"], ["claro", "Claro"]] as const;
  let theme = $state((typeof localStorage !== "undefined" && localStorage.getItem("agentia-theme")) || "grafito");
  let showThemes = $state(false);
  $effect(() => {
    document.documentElement.dataset.theme = theme;
    try { localStorage.setItem("agentia-theme", theme); } catch {}
  });

  // ── derived
  type Row = Task & { project: Project };
  const allTasks = $derived<Row[]>(snap ? snap.projects.flatMap((p) => p.tasks.map((t) => ({ ...t, project: p }))) : []);
  const current = $derived(allTasks.find((t) => t.path === selected) ?? null);
  const waiting = $derived(allTasks.filter((t) => t.status === "waiting"));
  const counts = $derived({
    todas: allTasks.filter((t) => !t.is_main).length,
    activas: allTasks.filter((t) => t.status === "working").length,
    necesitan: waiting.length,
    listas: allTasks.filter((t) => t.status === "done").length,
    dormidos: allTasks.filter((t) => t.status === "asleep").length,
  });
  const running = $derived(current ? !["idle", "asleep", "exited"].includes(current.status) : false);
  const extra = $derived(current ? (terms[current.path] ?? []) : []);
  const activeTerm = $derived(tab === "agente" ? selected : extra.find((t) => t.id === tab)?.id ?? null);

  // Stable color per project (from its name) so each section is recognizable at a glance
  const PROJ_COLORS = ["#0a84ff", "#30d158", "#ff9f0a", "#bf5af2", "#ff375f", "#64d2ff", "#ffd60a", "#ac8e68"];
  function projColor(name: string) {
    let h = 0;
    for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
    return PROJ_COLORS[h % PROJ_COLORS.length];
  }

  function visible(t: Task) {
    if (query && !`${t.title} ${t.branch}`.toLowerCase().includes(query.toLowerCase())) return false;
    if (filter === "activas") return t.status === "working";
    if (filter === "necesitan") return t.status === "waiting";
    if (filter === "listas") return t.status === "done";
    return true;
  }

  // ── data
  async function refresh() {
    try { snap = await api.snapshot(); } catch (e) { flash(String(e)); }
  }
  let toastTimer: ReturnType<typeof setTimeout>;
  function flash(msg: string) { toast = msg; clearTimeout(toastTimer); toastTimer = setTimeout(() => (toast = ""), 3800); }
  function setStatus(id: string, status: Status) {
    if (!snap) return;
    for (const p of snap.projects) for (const t of p.tasks) if (t.path === id) t.status = status;
  }

  async function notify(title: string, body: string) {
    try {
      if (await getCurrentWindow().isFocused()) return; // you're looking at the app already
      let ok = await isPermissionGranted();
      if (!ok) ok = (await requestPermission()) === "granted";
      if (ok) sendNotification({ title, body });
    } catch {}
  }

  // The webview zooms with Ctrl + scroll (easy to trigger by accident on a touchpad): always start at
  // 100 %, swallow Ctrl + wheel, and let Ctrl + 0 reset it.
  const resetZoom = () => getCurrentWebview().setZoom(1).catch(() => {});
  onMount(() => {
    resetZoom();
    const noZoom = (e: WheelEvent) => { if (e.ctrlKey) e.preventDefault(); };
    window.addEventListener("wheel", noZoom, { passive: false });
    refresh();
    const un = listen<{ id: string; status: Status }>("pty-status", (e) => {
      const t = allTasks.find((x) => x.path === e.payload.id);
      const before = t?.status;
      setStatus(e.payload.id, e.payload.status);
      if (!t || before === e.payload.status) return;
      const who = AGENTS[t.agent || "claude"].name;
      if (e.payload.status === "waiting") {
        if (e.payload.id !== selected) flash(`${t.title}: te necesita`);
        notify(`${who} te necesita`, `${t.project.name} · ${t.title}`);
      } else if (e.payload.status === "done" && before === "working") {
        notify(`${who} terminó`, `${t.project.name} · ${t.title}`);
      }
    });
    const iv = setInterval(refresh, 15000);
    return () => { un.then((f) => f()); clearInterval(iv); window.removeEventListener("wheel", noZoom); };
  });

  // Changes of the selected task (only this worktree, every 6 s)
  $effect(() => {
    const t = current?.path;
    changes = null;
    if (!t) return;
    const load = () => api.changes(t).then((c) => (changes = c)).catch(() => (changes = null));
    load();
    const iv = setInterval(load, 6000);
    return () => clearInterval(iv);
  });
  // Run scripts for the "+" menu
  $effect(() => {
    const t = current?.path;
    scripts = [];
    if (t) api.scripts(t).then((s) => (scripts = s)).catch(() => {});
  });

  function select(path: string) {
    if (path !== selected) { openFile = null; focusChange = null; }
    selected = path;
    tab = "agente";
    menu = false;
    renaming = false;
    view = "tareas";
  }

  // Terminal openers
  // Opening a task only attaches to an agent that is already running; starting one is an explicit click
  // ("Continuar"), so browsing tasks never spends RAM or plan usage.
  const openAgent = $derived(current ? () => api.openTask(current.path, false) : null);
  const openExtra = $derived.by(() => {
    const t = extra.find((x) => x.id === tab);
    return t && current ? () => api.openTerminal(t.id, current.path, t.command) : null;
  });

  async function startAgent() {
    if (!current) return;
    try { await api.openTask(current.path, true); await refresh(); termRef?.reload(); } catch (e) { flash(String(e)); }
  }
  function addTerminal(command: string) {
    if (!current) return;
    const list = terms[current.path] ?? [];
    const id = `${current.path}#${Date.now()}`;
    terms[current.path] = [...list, { id, label: command ? command.replace(/^(npm|pnpm|yarn|bun) run /, "") : "Terminal", command }];
    tab = id;
    showRun = false;
  }
  function closeTerminal(id: string) {
    if (!current) return;
    api.stop(id);
    terms[current.path] = (terms[current.path] ?? []).filter((t) => t.id !== id);
    if (tab === id) tab = "agente";
  }

  function send(text: string) {
    if (!selected) return;
    api.write(selected, text).catch((e) => flash(String(e)));
  }
  async function toggle(p: Project) {
    p.collapsed = !p.collapsed;
    await api.setCollapsed(p.id, p.collapsed);
  }
  async function act(fn: () => Promise<unknown>, ok: string) {
    try { await fn(); flash(ok); refresh(); } catch (e) { flash(String(e)); }
  }
  async function saveTitle() {
    if (current && renaming && newTitle.trim()) await act(() => api.rename(current!.path, newTitle.trim()), "Nombre actualizado");
    renaming = false;
  }
  async function removeTask() {
    if (!current || current.is_main) return;
    const ok = confirm(`¿Borrar la tarea "${current.title}"?\n\nSe detiene su agente y se borran su carpeta de trabajo y su rama ${current.branch}.\nLo que ya subiste con push sigue en GitHub.`);
    if (!ok) return;
    const path = current.path, project = current.project.id;
    selected = null;
    await act(() => api.deleteTask(project, path, true), "Tarea borrada");
  }
  function key(e: KeyboardEvent) {
    if (e.ctrlKey && e.key.toLowerCase() === "n") { e.preventDefault(); showNew = true; }
    if (e.ctrlKey && e.key.toLowerCase() === "k") { e.preventDefault(); document.getElementById("q")?.focus(); }
    if (e.key === "Escape") { menu = false; showRun = false; showThemes = false; }
    if (e.ctrlKey && (e.key === "0" || e.key === "+" || e.key === "=" || e.key === "-")) { e.preventDefault(); resetZoom(); }
  }
</script>

<svelte:window onkeydown={key} />

<div class="shell">
<div class="app" class:board={view === "tablero"}>
  <!-- Sidebar -->
  <aside class="side card">
    <div class="top">
      <img class="logo" src="/agentia-icon-small.svg" alt="" /><b class="brand">Agentia</b>
      <span class="grow"></span>
    </div>
    <label class="searchbox"><Search size={15} /><input id="q" bind:value={query} placeholder="Buscar" /><kbd>Ctrl K</kbd></label>
    <nav class="nav">
      <button class:on={view === "tareas"} onclick={() => (view = "tareas")}><ListTodo size={15} />Tareas{#if counts.activas}<span class="nc">{counts.activas}</span>{/if}</button>
      <button class:on={view === "tablero"} onclick={() => (view = "tablero")}><KanbanSquare size={15} />Tablero</button>
    </nav>
    <div class="sh">
      <b>Proyectos</b>
      <span class="grow"></span>
      <button class="hi" title="Te necesitan" onclick={() => (filter = filter === "necesitan" ? "todas" : "necesitan")}>
        <Bell size={15} />{#if counts.necesitan}<i class="hb">{counts.necesitan}</i>{/if}</button>
      <button class="hi" class:on={showFilters} title="Filtrar" onclick={() => (showFilters = !showFilters)}><SlidersHorizontal size={15} /></button>
      <button class="hi" title="Agregar proyecto" onclick={() => (showSettings = true)}><FolderPlus size={15} /></button>
      <button class="hi" title="Nueva tarea (Ctrl N)" onclick={() => (showNew = true)}><Plus size={16} /></button>
    </div>
    {#if showFilters || filter !== "todas"}
      <div class="filters">
        <button class:on={filter === "todas"} onclick={() => (filter = "todas")}>Todas <i>{counts.todas}</i></button>
        <button class:on={filter === "activas"} onclick={() => (filter = "activas")}><span style="color:var(--working)">●</span>Activas <i>{counts.activas}</i></button>
        <button class:on={filter === "necesitan"} onclick={() => (filter = "necesitan")}><span style="color:var(--waiting)">●</span>Te necesitan <i>{counts.necesitan}</i></button>
        <button class:on={filter === "listas"} onclick={() => (filter = "listas")}><span style="color:var(--done)">●</span>Listas <i>{counts.listas}</i></button>
      </div>
    {/if}

    {#if waiting.length}
      <div class="inbox">
        {#each waiting.slice(0, 2) as w (w.path)}
          <div class="ibox">
            <span class="label" style="color:var(--waiting)"><Bell size={11} /> Te necesita</span>
            <b>{w.title}</b>
            <span class="dim3 small">{w.project.name} · {AGENTS[w.agent || "claude"].name}</span>
            <div class="iact">
              <button class="yes" onclick={() => api.write(w.path, "\r")}>Sí</button>
              <button onclick={() => api.write(w.path, "\x1b")}>No</button>
              <button onclick={() => select(w.path)}>Abrir</button>
            </div>
          </div>
        {/each}
      </div>
    {/if}

    <div class="list">
      {#if snap && !snap.projects.length}
        <p class="empty dim">No hay proyectos todavía.<br /><button class="link" onclick={() => (showSettings = true)}>Agrega uno en Ajustes</button></p>
      {/if}
      {#each snap?.projects ?? [] as p (p.id)}
        {@const main = p.tasks.find((t) => t.is_main)}
        {@const tasks = p.tasks.filter((t) => !t.is_main && visible(t))}
        <section class="proj">
          <div class="ph">
            <button class="pmain" onclick={() => toggle(p)} title={p.path}>
              <span class="pic">
                {#if p.icon === "shopify"}<span class="shop" aria-label="Shopify"></span>
                {:else if p.icon}<img src={p.icon} alt="" />
                {:else}<FolderIcon size={15} />{/if}
              </span>
              <span class="pn">{p.name}</span>
            </button>
            {#if tasks.length && p.collapsed}<span class="pc">{tasks.length}</span>{/if}
          </div>
          {#if !p.collapsed}
            {#if p.error}<p class="perr">{p.error.includes("no es un repositorio") || p.error.includes("not a git repository") ? "No es un repositorio git" : p.error}</p>{/if}
            <div class="tasks">
              {#if main}
                <button class="task" class:sel={main.path === selected} onclick={() => select(main.path)}>
                  <i class="st" style="background:{main.status !== 'idle' ? STATUS[main.status].color : 'var(--text3)'}"></i>
                  <span class="tt"><span class="tr1"><span class="tn">{main.branch || "principal"}</span><span class="tag">principal</span></span></span>
                </button>
              {/if}
              {#each tasks as t (t.path)}
                <button class="task" class:sel={t.path === selected} onclick={() => select(t.path)}>
                  <i class="st" class:pulse={t.status === "working"} style="background:{t.status === 'idle' ? 'var(--done)' : STATUS[t.status].color}"></i>
                  <span class="tt">
                    <span class="tr1"><span class="tn">{t.title}</span>
                      {#if t.status !== "idle" && t.status !== "done"}<span class="pill" style="background:color-mix(in srgb, {STATUS[t.status].color} 16%, transparent);color:{STATUS[t.status].color}">{STATUS[t.status].label}</span>{/if}</span>
                    <span class="tr2 mono">{t.branch || "—"}</span>
                    {#if t.session?.text || t.agent}
                      <span class="tr3">
                        {#if t.status === "done"}<CheckCircle2 size={12} class="ok" />{/if}
                        <AgentLogo id={t.agent || "claude"} size={12} />
                        <span class="sx">{t.session?.text || AGENTS[t.agent || "claude"].name}</span>
                        {#if t.session?.at}<span class="dim3">{ago(t.session.at)}</span>{/if}
                      </span>
                    {/if}
                  </span>
                </button>
              {/each}
              {#if !tasks.length && !p.error && !query && filter === "todas"}
                <button class="addtask" onclick={() => (showNew = true)}><Plus size={12} />Nueva tarea</button>
              {/if}
            </div>
          {/if}
        </section>
      {/each}
    </div>
    <div class="sfoot">
      <button class="hi" title="Ajustes" onclick={() => (showSettings = true)}><Settings2 size={15} /></button>
      <div class="tw">
        <button class="hi" title="Tema" onclick={() => (showThemes = !showThemes)}><Palette size={15} /></button>
        {#if showThemes}
          <div class="themes card">
            {#each THEMES as [id, name]}
              <button class:on={theme === id} onclick={() => { theme = id; showThemes = false; }}><i data-t={id}></i>{name}</button>
            {/each}
          </div>
        {/if}
      </div>
      <span class="grow"></span>
      <button class="newbtn" onclick={() => (showNew = true)} disabled={!snap?.projects.length}><Plus size={14} />Nueva tarea</button>
    </div>
    <div class="sstatus"><UsageBar /></div>
  </aside>

  {#if view === "tablero"}
    <main class="center card wide">
      <Board projects={snap?.projects ?? []} onOpen={select} onChanged={refresh} />
    </main>
  {:else}

  <!-- Center -->
  <main class="center card">
    {#if current}
      {@const a = AGENTS[current.agent || "claude"]}
      <header class="head">
        <div class="crumbs dim3 small">{current.project.name} › {current.is_main ? "Carpeta principal" : "Tarea"}</div>
        <div class="h1">
          {#if renaming}
            <!-- svelte-ignore a11y_autofocus -->
            <input class="ren" bind:value={newTitle} autofocus onkeydown={(e) => { if (e.key === "Enter") saveTitle(); if (e.key === "Escape") renaming = false; }} onblur={saveTitle} />
          {:else}
            <b role="button" tabindex="0" title="Doble click para renombrar" ondblclick={() => { if (!current.is_main) { newTitle = current.title; renaming = true; } }}>{current.is_main ? current.project.name : current.title}</b>
          {/if}
          <span class="pill" style="background:color-mix(in srgb, {STATUS[current.status].color} 16%, transparent);color:{STATUS[current.status].color}">{STATUS[current.status].label}</span>
          <span class="grow"></span>
          <button class="btn" title="Abrir la carpeta en tu editor" onclick={() => act(() => api.openInEditor(current!.path), "Abriendo en tu editor…")}><Code2 size={14} /><span>Editor</span></button>
          {#if running}
            <button class="btn" title="Dormir: libera su RAM, la conversación se guarda" onclick={() => act(() => api.sleep(current!.path), "Agente dormido: su RAM quedó libre")}><Moon size={14} /><span>Dormir</span></button>
          {:else if current.status === "asleep"}
            <button class="btn" onclick={startAgent}><Sun size={14} /><span>Despertar</span></button>
          {/if}
          {#if !current.is_main}
            <button class="btn primary" onclick={() => { tab = "cambios"; setTimeout(() => changesRef?.openPr(current!.title), 60); }}><GitPullRequest size={14} /><span>Crear PR</span></button>
          {/if}
          <div class="mw">
            <button class="btn icon" aria-label="Más" onclick={() => (menu = !menu)}><MoreHorizontal size={15} /></button>
            {#if menu}
              <div class="menu card">
                {#if !current.is_main}<button onclick={() => { menu = false; newTitle = current!.title; renaming = true; }}><Pencil size={13} />Renombrar</button>{/if}
                {#if running}<button onclick={() => { menu = false; act(() => api.stop(current!.path), "Agente detenido"); }}><Square size={13} />Detener agente</button>{/if}
                {#if !current.is_main}<button class="danger" onclick={() => { menu = false; removeTask(); }}><Trash2 size={13} />Borrar tarea</button>{/if}
                {#if current.is_main && !running}<button onclick={() => (menu = false)}><X size={13} />Nada que hacer aquí</button>{/if}
              </div>
            {/if}
          </div>
        </div>
        <div class="meta">
          {#if current.agent || !current.is_main}<span class="chip"><AgentLogo id={current.agent || "claude"} size={13} />{a.name}{current.model ? ` · ${current.model}` : ""}</span>{/if}
          <span class="chip"><GitBranch size={12} />{current.branch || "—"}{#if changes && !current.is_main} → {changes.base}{/if}</span>
          <span class="chip mono"><Folder size={12} />{current.path.replace(/^\/home\/[^/]+/, "~")}</span>
          {#if changes?.files.length}<span class="chip"><span class="add">+{changes.added}</span><span class="del">−{changes.removed}</span></span>{/if}
        </div>
        <div class="tabs">
          <button class:on={tab === "agente"} onclick={() => (tab = "agente")}><TermIcon size={14} />Agente</button>
          {#each extra as t (t.id)}
            <span class="tabx" class:on={tab === t.id}>
              <button onclick={() => (tab = t.id)}><Play size={12} />{t.label}</button>
              <button class="tx" aria-label="Cerrar" onclick={() => closeTerminal(t.id)}><X size={11} /></button>
            </span>
          {/each}
          <div class="mw">
            <button class="plus" title="Abrir una terminal o un comando de tu proyecto" onclick={() => (showRun = !showRun)}><Play size={12} />Comando</button>
            {#if showRun}
              <div class="menu card left">
                <button onclick={() => addTerminal("")}><TermIcon size={13} />Terminal</button>
                {#each scripts.slice(0, 8) as sc}<button class="mono" onclick={() => addTerminal(sc)}><Play size={12} />{sc}</button>{/each}
              </div>
            {/if}
          </div>
          <button class:on={tab === "cambios"} onclick={() => (tab = "cambios")}><FileDiff size={14} />Cambios
            {#if changes?.files.length}<i>{changes.files.length}</i>{/if}</button>
          {#if openFile}
            <span class="tabx" class:on={tab === "archivo"}>
              <button onclick={() => (tab = "archivo")}><FileText size={12} />{openFile.split("/").pop()}</button>
              <button class="tx" aria-label="Cerrar" onclick={() => { openFile = null; if (tab === "archivo") tab = "agente"; }}><X size={11} /></button>
            </span>
          {/if}
        </div>
      </header>

      <div class="termbox" class:hidden={tab === "cambios" || tab === "archivo"}>
        <Terminal bind:this={termRef} termId={activeTerm} open={tab === "agente" ? openAgent : openExtra} />
        {#if tab === "agente" && !running && current.status !== "waiting"}
          <div class="start">
            {#if current.status === "asleep"}
              <Moon size={26} /><b>Agente dormido</b><p class="dim">Se durmió para liberar RAM. Su conversación está guardada.</p>
              <button class="btn blue" onclick={startAgent}><Sun size={14} />Despertar y continuar</button>
            {:else if current.is_main}
              <TermIcon size={26} /><b>Carpeta principal de {current.project.name}</b><p class="dim">Aquí no arranca ningún agente solo. Para trabajar sin tocar tu rama principal, crea una tarea.</p>
              <div class="row"><button class="btn blue" onclick={() => (showNew = true)}><Plus size={14} />Nueva tarea</button>
                <button class="btn" onclick={startAgent}>Abrir agente aquí</button></div>
            {:else}
              <Play size={26} /><b>El agente no está corriendo</b><p class="dim">Retoma su última conversación en esta rama.</p>
              <button class="btn blue" onclick={startAgent}><Play size={14} />Continuar</button>
            {/if}
          </div>
        {/if}
      </div>
      {#if tab === "cambios"}
        <ChangesView bind:this={changesRef} path={current.path} {changes} focusFile={focusChange} onChanged={() => api.changes(current!.path).then((c) => (changes = c))} onToast={flash} />
      {/if}
      {#if tab === "archivo" && openFile}
        <FileView path={current.path} file={openFile} onEditor={() => act(() => api.openInEditor(current!.path), "Abriendo en tu editor…")} />
      {/if}
    {:else}
      <div class="welcome">
        <img src="/agentia-icon.svg" alt="" width="96" height="96" />
        <div class="big">Agentia</div>
        <p class="dim">Dirige a todas tus IAs. Sin comerte la RAM.</p>
        <p class="dim3">Elige una tarea a la izquierda o crea una nueva con <kbd>Ctrl N</kbd>.</p>
      </div>
    {/if}
  </main>

  <Inspector path={current?.path ?? null} {changes} active={current?.status === "working"}
    onOpenFile={(f) => { openFile = f; tab = "archivo"; }}
    onOpenChange={(f) => { focusChange = f; tab = "cambios"; }} />
  {/if}
</div>


</div>

{#if showNew && snap}
  <NewTask projects={snap.projects} initialProject={current?.project.id ?? ""} defaultAgent={snap.settings.default_agent}
    onClose={() => (showNew = false)}
    onCreated={async (path) => { showNew = false; await refresh(); select(path); flash("Tarea creada y agente en marcha"); }} />
{/if}
{#if showSettings && snap}
  <SettingsSheet settings={snap.settings} projects={snap.projects} onClose={() => (showSettings = false)} onSaved={refresh} onToast={flash} />
{/if}

{#if toast}<div class="toast">{toast}</div>{/if}

<style>
  .shell { height: 100vh; display: flex; flex-direction: column; }
  .app { flex: 1; min-height: 0; display: grid; grid-template-columns: 280px 1fr 300px; gap: 8px; padding: 0 8px 0 0; }
  .app.board { grid-template-columns: 280px 1fr; }
  .app > :global(main), .app > :global(.insp) { margin: 8px 0; }
  /* sidebar: a flush full-height panel (not a floating card), like a native source list */
  .side.card { border-radius: 0; border: 0; border-right: 1px solid var(--line); box-shadow: none; background: var(--s1); }
  .searchbox { margin: 4px 10px 8px; height: 34px; border-radius: 10px; background: var(--s3); display: flex; align-items: center; gap: 8px; padding: 0 10px; color: var(--text3); }
  .searchbox input { flex: 1; min-width: 0; background: none; border: 0; outline: none; font-size: 13px; }
  .searchbox:focus-within { box-shadow: 0 0 0 2px color-mix(in srgb, var(--working) 60%, transparent); }
  .sstatus { padding: 4px 6px 6px; border-top: 1px solid var(--line); }
  .top { display: flex; align-items: center; gap: 9px; padding: 12px 14px 6px; }
  .top .logo { width: 24px; height: 24px; margin: 0; }
  .brand { font-family: var(--display); font-size: 15px; }
  .nav { display: flex; flex-direction: column; padding: 0 8px 8px; gap: 1px; }
  .nav button { display: flex; align-items: center; gap: 10px; padding: 7px 10px; border-radius: 9px; font-size: 13px; color: var(--text2); }
  .nav button:hover { background: var(--s2); color: var(--text); }
  .nav button.on { background: var(--s3); color: var(--text); }

  .nc { margin-left: auto; font-size: 10.5px; font-weight: 800; color: var(--working); }
  .sfoot { display: flex; align-items: center; gap: 4px; padding: 8px 10px; border-top: 1px solid var(--line); }
  .grow { flex: 1; }
  .small { font-size: 11.5px; }
  kbd { font-family: var(--font); font-size: 10px; border: 1px solid var(--line2); border-radius: 5px; padding: 0 5px; color: var(--text3); }
  .link { color: var(--working); font-weight: 700; }
  /* rail */
  .tw { position: relative; }
  .themes { position: absolute; left: 0; bottom: 34px; z-index: 10; padding: 6px; display: flex; flex-direction: column; gap: 2px; min-width: 150px; }
  .themes button { display: flex; align-items: center; gap: 9px; padding: 7px 10px; border-radius: 8px; font-weight: 600; text-align: left; }
  .themes button:hover, .themes button.on { background: var(--s3); }
  .themes i { width: 16px; height: 16px; border-radius: 5px; border: 1px solid #ffffff30; }
  .themes i[data-t="grafito"] { background: #2c2c2e; } .themes i[data-t="medianoche"] { background: #1c2236; }
  .themes i[data-t="negro"] { background: #030304; } .themes i[data-t="claro"] { background: #f2f2f6; }
  /* sidebar */
  .side { display: flex; flex-direction: column; overflow: hidden; }
  .sh { display: flex; gap: 4px; align-items: center; padding: 10px 10px 4px 16px; border-top: 1px solid var(--line); }
  .sh b { font-size: 12px; font-weight: 700; color: var(--text3); }
  .filters { display: flex; flex-wrap: wrap; gap: 5px; padding: 0 12px 10px; }
  .filters button { font-size: 11px; font-weight: 700; padding: 4px 8px; border-radius: 8px; background: var(--s2); color: var(--text2); display: flex; gap: 4px; align-items: center; }
  .filters button.on { background: var(--text); color: var(--bg); }
  .filters i { font-style: normal; opacity: .6; }
  .inbox { padding: 0 10px 8px; display: flex; flex-direction: column; gap: 6px; }
  .ibox { border: 1px solid #ffd60a55; background: linear-gradient(180deg, #ffd60a14, transparent); border-radius: 12px; padding: 10px; display: flex; flex-direction: column; gap: 3px; }
  .ibox .label { display: flex; gap: 4px; align-items: center; }
  .iact { display: flex; gap: 6px; margin-top: 6px; }
  .iact button { font-size: 11.5px; font-weight: 700; padding: 4px 12px; border-radius: 8px; background: var(--s3); }
  .iact .yes { background: var(--waiting); color: #000; }
  .list { flex: 1; overflow-y: auto; padding: 2px 0 8px; }
  .empty { padding: 30px 16px; text-align: center; line-height: 1.6; }
  .sh { align-items: center !important; }
  .hi { position: relative; width: 26px; height: 26px; border-radius: 7px; display: flex; align-items: center; justify-content: center; color: var(--text3); }
  .hi:hover, .hi.on { background: var(--s3); color: var(--text); }
  .hb { position: absolute; top: -2px; right: -3px; min-width: 14px; height: 14px; border-radius: 7px; background: var(--waiting); color: #000; font-size: 9px; font-weight: 800; font-style: normal; display: flex; align-items: center; justify-content: center; padding: 0 3px; }
  .proj { margin: 2px 8px; }
  .ph { display: flex; align-items: center; }
  .pmain { flex: 1; min-width: 0; display: flex; align-items: center; gap: 9px; padding: 6px 8px; border-radius: 9px; text-align: left; }
  .pmain:hover { background: var(--s2); }
  .pic { width: 18px; height: 18px; flex: none; display: flex; align-items: center; justify-content: center; color: var(--text3); }
  .pic img { width: 18px; height: 18px; border-radius: 4px; object-fit: contain; }
  .pic .shop { width: 16px; height: 16px; background: #95bf47; -webkit-mask: url('/agents/shopify.svg') center / contain no-repeat; mask: url('/agents/shopify.svg') center / contain no-repeat; }
  .pn { font-size: 13px; font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pc { font-size: 11px; font-weight: 700; color: var(--text3); padding: 0 8px; }
  .perr { font-size: 11px; color: var(--text3); font-style: italic; padding: 0 0 6px 35px; }
  .tasks { display: flex; flex-direction: column; gap: 2px; padding-bottom: 6px; }
  .task { width: 100%; display: flex; gap: 10px; align-items: flex-start; padding: 7px 10px; border-radius: 10px; text-align: left; border: 1px solid transparent; }
  .task:hover { background: var(--s2); }
  .task.sel { background: var(--s3); border-color: var(--line2); box-shadow: 0 1px 0 #ffffff08 inset; }
  .st { width: 8px; height: 8px; border-radius: 4px; flex: none; margin-top: 5px; }
  .st.pulse { box-shadow: 0 0 0 0 var(--working); animation: pulse 2s ease-out 3; } /* a few pulses, then still: no constant redraw */
  @keyframes pulse { to { box-shadow: 0 0 0 6px transparent; } }
  .tt { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .tr1 { display: flex; align-items: center; gap: 7px; }
  .tn { font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tag { font-size: 10.5px; font-weight: 700; color: var(--text2); border: 1px solid var(--line2); border-radius: 6px; padding: 0 6px; }
  .tr2 { font-size: 11px; color: var(--text3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tr3 { display: flex; align-items: center; gap: 6px; font-size: 11.5px; color: var(--text2); margin-top: 2px; }
  .tr3 :global(.ok) { color: var(--done); flex: none; }
  .sx { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .addtask { margin-left: 18px; display: flex; align-items: center; gap: 5px; font-size: 11.5px; color: var(--text3); padding: 4px 8px; border-radius: 7px; white-space: nowrap; }
  .addtask:hover { color: var(--text); background: var(--s2); }
  .newbtn { height: 30px; padding: 0 12px; border-radius: 9px; background: linear-gradient(135deg, var(--working), #5e5ce6);
    display: flex; align-items: center; justify-content: center; gap: 5px; font-weight: 700; font-size: 12.5px; color: #fff; }
  .newbtn:disabled { opacity: .4; }
  /* center */
  .center { display: flex; flex-direction: column; overflow: hidden; min-width: 0; }
  .head { padding: 12px 16px 0; border-bottom: 1px solid var(--line); }
  .h1 { display: flex; align-items: center; gap: 8px; margin-top: 4px; }
  .h1 b { font-family: var(--display); font-size: 21px; letter-spacing: -.3px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 120px; flex: 0 1 auto; cursor: text; }
  .h1 .btn { flex: none; }
  .btn.icon { padding: 0 8px; }
  .ren { font-family: var(--display); font-size: 19px; font-weight: 700; background: var(--s2); border: 1px solid var(--working); border-radius: 8px; padding: 2px 8px; outline: none; min-width: 260px; user-select: text; }
  .mw { position: relative; }
  .menu { position: absolute; right: 0; top: 36px; z-index: 12; padding: 5px; min-width: 190px; display: flex; flex-direction: column; box-shadow: 0 16px 40px #0008; }
  .menu.left { left: 0; right: auto; top: 32px; }
  .menu button { display: flex; align-items: center; gap: 8px; padding: 7px 10px; border-radius: 7px; text-align: left; font-size: 12.5px; font-weight: 600; white-space: nowrap; }
  .menu button:hover { background: var(--s3); }
  .menu .danger { color: var(--danger); }
  .meta { display: flex; gap: 6px; margin-top: 9px; flex-wrap: wrap; }
  .chip { display: inline-flex; align-items: center; gap: 5px; font-size: 11.5px; font-weight: 700; padding: 4px 9px; border-radius: 9px; background: var(--s2); border: 1px solid var(--line2); color: var(--text2); }
  .tabs { display: flex; gap: 2px; margin-top: 10px; align-items: center; }
  .tabs > button, .tabx > button:first-child { font-size: 12.5px; font-weight: 600; padding: 8px 11px; color: var(--text2); display: flex; gap: 6px; align-items: center; border-bottom: 2px solid transparent; }
  .tabs > button.on, .tabx.on > button:first-child { color: var(--text); border-color: var(--working); }
  .tabx { display: flex; align-items: center; }
  .tabx .tx { padding: 3px; border-radius: 5px; color: var(--text3); }
  .tabx .tx:hover { background: var(--s3); color: var(--text); }
  .plus { height: 26px; padding: 0 9px; border-radius: 7px; display: flex; align-items: center; gap: 5px; font-size: 12px; font-weight: 600; color: var(--text3); }
  .plus:hover { background: var(--s3); color: var(--text); }
  .tabs i { font-style: normal; font-size: 10.5px; background: var(--s3); border-radius: 6px; padding: 0 6px; }
  .termbox { position: relative; flex: 1; min-height: 0; margin: 8px 10px 10px; border-radius: 12px; overflow: hidden; border: 1px solid var(--line2); }
  .start { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; background: color-mix(in srgb, var(--term) 92%, transparent); text-align: center; padding: 20px; }
  .start b { font-size: 15px; } .start p { max-width: 380px; line-height: 1.45; }
  .start .row { display: flex; gap: 8px; margin-top: 4px; }
  .hidden { display: none !important; }
  .welcome { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; }
  .big { font-family: var(--display); font-size: 44px; font-weight: 800; letter-spacing: -1px; }
  /* status bar */
  .toast { position: fixed; bottom: 22px; left: 50%; transform: translateX(-50%); background: var(--s1); border: 1px solid var(--line2);
    padding: 10px 16px; border-radius: 12px; font-weight: 600; z-index: 30; box-shadow: 0 10px 30px #000a; }
  /* narrow windows */
  @media (max-width: 1350px) { .app { grid-template-columns: 270px 1fr; } .app :global(.insp) { display: none; } }
  @media (max-width: 1100px) { .app { grid-template-columns: 240px 1fr; } .h1 .btn span { display: none; } }
</style>
