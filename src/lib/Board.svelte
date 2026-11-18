<script lang="ts">
  // Tablero: tasks as cards in columns; drag a card to move it, click to open it.
  import { api, AGENTS, STATUS, ago, type Project, type Task } from "./api";
  import AgentLogo from "./AgentLogo.svelte";

  let { projects, onOpen, onChanged }: { projects: Project[]; onOpen: (path: string) => void; onChanged: () => void } = $props();

  const COLUMNS = [
    { id: "por-hacer", name: "Por hacer", color: "var(--text3)" },
    { id: "trabajando", name: "Trabajando", color: "var(--working)" },
    { id: "revision", name: "En revisión", color: "var(--copilot)" },
    { id: "hecho", name: "Hecho", color: "var(--done)" },
  ];
  let filter = $state("todos");
  let dragging = $state<string | null>(null);
  let over = $state<string | null>(null);

  const tasks = $derived(
    projects
      .filter((p) => filter === "todos" || p.id === filter)
      .flatMap((p) => p.tasks.filter((t) => !t.is_main).map((t) => ({ ...t, project: p }))),
  );
  function col(t: Task) {
    if (t.status === "working" || t.status === "waiting") return "trabajando";
    return t.column || "trabajando";
  }
  async function drop(column: string) {
    const p = dragging;
    dragging = null; over = null;
    if (!p) return;
    await api.setColumn(p, column);
    onChanged();
  }
</script>

<div class="board">
  <div class="bh">
    <b>Tablero</b>
    <div class="pf">
      <button class:on={filter === "todos"} onclick={() => (filter = "todos")}>Todos</button>
      {#each projects as p (p.id)}<button class:on={filter === p.id} onclick={() => (filter = p.id)}>{p.name}</button>{/each}
    </div>
  </div>
  <div class="cols">
    {#each COLUMNS as c (c.id)}
      {@const list = tasks.filter((t) => col(t) === c.id)}
      <div class="col" class:over={over === c.id} role="list"
        ondragover={(e) => { e.preventDefault(); over = c.id; }} ondragleave={() => (over = null)} ondrop={() => drop(c.id)}>
        <h5><i style="background:{c.color}"></i>{c.name}<span class="dim3">{list.length}</span></h5>
        {#each list as t (t.path)}
          {@const a = AGENTS[t.agent || "claude"]}
          <button class="tcard" draggable="true" ondragstart={() => (dragging = t.path)} onclick={() => onOpen(t.path)}
            style="--sc:{STATUS[t.status].color}" class:live={t.status === "working" || t.status === "waiting"}>
            <span class="pj dim3"><span class="ra">{t.project.name[0]}</span>{t.project.name}</span>
            <b>{t.title}</b>
            <span class="meta"><AgentLogo id={t.agent || "claude"} size={13} />{a.name}
              {#if t.status !== "idle"}<span class="pill" style="background:color-mix(in srgb, {STATUS[t.status].color} 16%, transparent);color:{STATUS[t.status].color}">{STATUS[t.status].label}</span>{/if}
              <span class="dim3 when">{ago(t.last_activity)}</span></span>
          </button>
        {/each}
        {#if !list.length}<p class="dim3 empty">Arrastra tareas aquí</p>{/if}
      </div>
    {/each}
  </div>
</div>

<style>
  .board { flex: 1; min-height: 0; display: flex; flex-direction: column; padding: 14px 16px; gap: 12px; }
  .bh { display: flex; align-items: center; gap: 14px; }
  .bh b { font-family: var(--display); font-size: 21px; }
  .pf { display: flex; gap: 5px; flex-wrap: wrap; }
  .pf button { font-size: 11.5px; font-weight: 700; padding: 4px 10px; border-radius: 8px; background: var(--s2); color: var(--text2); }
  .pf button.on { background: var(--text); color: var(--bg); }
  .cols { flex: 1; min-height: 0; display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; }
  .col { background: var(--s1); border: 1px solid var(--line); border-radius: 14px; padding: 10px; overflow-y: auto; transition: border-color .15s; }
  .col.over { border-color: var(--working); }
  h5 { display: flex; align-items: center; gap: 7px; font-size: 12.5px; margin-bottom: 10px; }
  h5 i { width: 8px; height: 8px; border-radius: 4px; }
  h5 span { margin-left: auto; }
  .tcard { width: 100%; text-align: left; display: flex; flex-direction: column; gap: 5px; background: var(--s2); border: 1px solid var(--line2); border-radius: 11px; padding: 10px; margin-bottom: 8px; cursor: grab; }
  .tcard.live { box-shadow: inset 3px 0 0 var(--sc); }
  .tcard b { font-size: 13px; }
  .pj { display: flex; align-items: center; gap: 6px; font-size: 11px; }
  .ra { width: 16px; height: 16px; border-radius: 5px; background: #0a84ff26; color: #64a8ff; font-size: 9px; font-weight: 800; display: flex; align-items: center; justify-content: center; }
  .meta { display: flex; align-items: center; gap: 6px; font-size: 11.5px; color: var(--text2); }
  .when { margin-left: auto; }
  .empty { text-align: center; padding: 20px 0; font-size: 11.5px; }
</style>
