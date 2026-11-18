<script lang="ts">
  // Explorer: the task's files (git-tracked + new, .gitignore respected) as a folder tree.
  import { api } from "./api";
  import { ChevronRight, ChevronDown, Folder, FolderOpen, FileText } from "@lucide/svelte";

  let { path, changed, onOpen }: { path: string; changed: Set<string>; onOpen: (file: string) => void } = $props();

  type Node = { name: string; full: string; dir: boolean; kids: Node[] };
  let files = $state<string[]>([]);
  let open = $state<Set<string>>(new Set());
  let filter = $state("");

  $effect(() => {
    const p = path;
    files = []; open = new Set();
    api.listFiles(p).then((f) => (files = f)).catch(() => (files = []));
  });

  const tree = $derived.by(() => {
    const root: Node = { name: "", full: "", dir: true, kids: [] };
    const q = filter.toLowerCase();
    for (const f of files) {
      if (q && !f.toLowerCase().includes(q)) continue;
      let cur = root;
      const parts = f.split("/");
      parts.forEach((part, i) => {
        const full = parts.slice(0, i + 1).join("/");
        const dir = i < parts.length - 1;
        let n = cur.kids.find((k) => k.name === part && k.dir === dir);
        if (!n) { n = { name: part, full, dir, kids: [] }; cur.kids.push(n); }
        cur = n;
      });
    }
    const sort = (n: Node) => { n.kids.sort((a, b) => (a.dir === b.dir ? a.name.localeCompare(b.name) : a.dir ? -1 : 1)); n.kids.forEach(sort); };
    sort(root);
    return root;
  });

  function toggle(full: string) {
    const s = new Set(open);
    s.has(full) ? s.delete(full) : s.add(full);
    open = s;
  }
  const isOpen = (full: string) => open.has(full) || filter.length > 1;
  const dirChanged = (full: string) => [...changed].some((c) => c.startsWith(full + "/"));
</script>

<div class="ft">
  <input class="flt" bind:value={filter} placeholder="Filtrar archivos…" />
  <div class="rows">
    {#snippet branch(nodes: Node[], depth: number)}
      {#each nodes.slice(0, 400) as n (n.full)}
        {#if n.dir}
          <button class="row" style="padding-left:{6 + depth * 12}px" onclick={() => toggle(n.full)}>
            {#if isOpen(n.full)}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
            {#if isOpen(n.full)}<FolderOpen size={13} class="fi" />{:else}<Folder size={13} class="fi" />{/if}
            <span class="nm" class:ch={dirChanged(n.full)}>{n.name}</span>
          </button>
          {#if isOpen(n.full)}{@render branch(n.kids, depth + 1)}{/if}
        {:else}
          <button class="row" style="padding-left:{18 + depth * 12}px" onclick={() => onOpen(n.full)} title={n.full}>
            <FileText size={13} class="fi" /><span class="nm" class:ch={changed.has(n.full)}>{n.name}</span>
            {#if changed.has(n.full)}<i class="dot"></i>{/if}
          </button>
        {/if}
      {/each}
    {/snippet}
    {@render branch(tree.kids, 0)}
    {#if !files.length}<p class="dim3 empty">Sin archivos</p>{/if}
  </div>
</div>

<style>
  .ft { display: flex; flex-direction: column; min-height: 0; flex: 1; }
  .flt { margin: 0 0 8px; background: var(--s2); border: 1px solid var(--line2); border-radius: 8px; padding: 6px 9px; outline: none; font-size: 12px; user-select: text; }
  .rows { overflow-y: auto; min-height: 0; flex: 1; }
  .row { width: 100%; display: flex; align-items: center; gap: 5px; padding: 3px 6px; border-radius: 6px; text-align: left; font-size: 12.5px; color: var(--text2); }
  .row:hover { background: var(--s2); color: var(--text); }
  .row :global(.fi) { color: var(--text3); flex: none; }
  .nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .nm.ch { color: #ff9f0a; }
  .dot { width: 6px; height: 6px; border-radius: 3px; background: #ff9f0a; margin-left: auto; flex: none; }
  .empty { padding: 14px; font-size: 12px; }
</style>
