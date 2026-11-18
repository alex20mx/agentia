<script lang="ts">
  // Read-only file viewer with line numbers (center pane).
  import { api, type FileContent } from "./api";
  import { Code2 } from "@lucide/svelte";

  let { path, file, onEditor }: { path: string; file: string; onEditor: () => void } = $props();
  let content = $state<FileContent | null>(null);
  let error = $state("");

  $effect(() => {
    const p = path, f = file;
    content = null; error = "";
    api.readFile(p, f).then((c) => (content = c)).catch((e) => (error = String(e)));
  });
  const lines = $derived(content?.text.split("\n").slice(0, 20000) ?? []);
  const isImage = $derived(/\.(png|jpe?g|gif|webp|svg|ico)$/i.test(file));
</script>

<div class="fv">
  <div class="fh"><span class="mono">{file}</span>
    {#if content}<span class="dim3">{(content.size / 1024).toFixed(1)} KB</span>{/if}
    <span class="grow"></span>
    <button class="btn" onclick={onEditor}><Code2 size={13} />Abrir en editor</button></div>
  <div class="body mono">
    {#if error}<p class="msg">{error}</p>
    {:else if !content}<p class="msg dim3">Cargando…</p>
    {:else if content.binary}<p class="msg dim3">{isImage ? "Imagen" : "Archivo binario"} — ábrelo en tu editor para verlo.</p>
    {:else}
      {#each lines as l, i (i)}<div class="ln"><span class="no">{i + 1}</span><span class="tx">{l || " "}</span></div>{/each}
      {#if content.truncated}<p class="msg dim3">Archivo muy grande: se muestra el primer MB.</p>{/if}
    {/if}
  </div>
</div>

<style>
  .fv { flex: 1; min-height: 0; display: flex; flex-direction: column; margin: 8px 10px 10px; border: 1px solid var(--line2); border-radius: 12px; overflow: hidden; background: var(--term); }
  .fh { display: flex; align-items: center; gap: 10px; padding: 7px 10px 7px 14px; border-bottom: 1px solid var(--line); font-size: 12px; color: var(--text2); background: var(--s1); }
  .grow { flex: 1; }
  .body { flex: 1; overflow: auto; padding: 6px 0; font-size: 12.5px; line-height: 1.55; user-select: text; color: #d1d1d6; }
  .ln { display: flex; white-space: pre; }
  .no { width: 52px; flex: none; text-align: right; padding-right: 14px; color: #6e6e78; user-select: none; }
  .tx { padding-right: 14px; }
  .msg { padding: 24px; font-family: var(--font); }
</style>
