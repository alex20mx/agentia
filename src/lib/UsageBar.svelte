<script lang="ts">
  // Plan usage, Orca-style: a discreet "Usado ✳ 90% 1 h 18 m" line in the status bar; click for the
  // "Uso · todos los agentes" panel (Compacto / Detallado). Polls every 15 min (providers rate-limit
  // this), or on demand with the refresh button.
  import { onMount } from "svelte";
  import { api, PROVIDERS, untilReset, type ProviderUsage } from "./api";
  import AgentLogo from "./AgentLogo.svelte";
  import { RefreshCw, ChevronRight } from "@lucide/svelte";

  let data = $state<ProviderUsage[]>([]);
  let loading = $state(false);
  let open = $state(false);
  let mode = $state<"compacto" | "detallado">("compacto");
  let expanded = $state<string | null>(null);
  let updated = $state(0);

  async function load() {
    if (loading) return;
    loading = true;
    try { data = await api.usage(); updated = Date.now(); } catch {} finally { loading = false; }
  }
  onMount(() => {
    load();
    const iv = setInterval(load, 15 * 60 * 1000);
    return () => clearInterval(iv);
  });

  const tone = (u: number) => (u >= 90 ? "var(--danger)" : u >= 70 ? "var(--waiting)" : "var(--done)");
  const short = (s: string) => s.replace(" h ", "h ").replace(/ min$/, "m").replace(" días", "d");
  const name = (id: string) => PROVIDERS[id]?.name ?? id;
  // "Gemini Models · Semana" → "Gemini semana"
  const winLabel = (l: string) => l.replace(" Models", "").replace(" · ", " ").replace("Semana", "semana");
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && (open = false)} />

<div class="usage">
  <button class="line" onclick={() => (open = !open)} title="Uso de tus planes">
    <span class="dim3">Usado</span>
    {#each data.filter((p) => p.windows.length) as p (p.provider)}
      {@const w = p.windows[0]}
      <span class="item" title="{PROVIDERS[p.provider]?.name ?? p.provider} · {w.label}{w.resets_at ? ` · se reinicia en ${untilReset(w.resets_at)}` : ''}"><AgentLogo id={p.provider} size={12} />
        <b>{Math.round(w.used)}%</b>
        {#if p.provider === "claude" && w.resets_at}<span class="dim3 nowrap">{short(untilReset(w.resets_at))}</span>{/if}</span>
    {/each}
    {#if !data.length && !loading}<span class="dim3">—</span>{/if}
  </button>
  <button class="refresh" class:spin={loading} onclick={load} title={updated ? `Actualizado ${new Date(updated).toLocaleTimeString()}` : "Actualizar"}><RefreshCw size={11} /></button>

  {#if open}
    <div class="scrim" role="presentation" onclick={() => (open = false)}></div>
    <div class="pop card">
      <div class="ph"><b>Uso</b><span class="dim3">· todos los agentes</span>
        <button class="rf" class:spin={loading} onclick={load} aria-label="Actualizar"><RefreshCw size={13} /></button>
        <span class="grow"></span><span class="dim3 small">Usado</span></div>
      <div class="seg">
        <button class:on={mode === "detallado"} onclick={() => (mode = "detallado")}>Detallado</button>
        <button class:on={mode === "compacto"} onclick={() => (mode = "compacto")}>Compacto</button>
      </div>
      <div class="rows">
        {#each data as p (p.provider)}
          {@const w = p.windows[0]}
          <button class="row" onclick={() => (expanded = expanded === p.provider ? null : p.provider)}>
            <span class="lg"><AgentLogo id={p.provider} size={18} /></span>
            <span class="nm">{name(p.provider)}{#if p.plan}<small class="dim3"> {p.plan}</small>{/if}</span>
            {#if p.error}<span class="dim3 small">{p.error}</span>
            {:else if w}
              <span class="dim3 small">{untilReset(w.resets_at)}</span>
              <span class="track"><b style="width:{Math.min(100, w.used)}%;background:{tone(w.used)}"></b></span>
              <span class="pct">{Math.round(w.used)}%</span>
            {/if}
            <ChevronRight size={14} class={expanded === p.provider || mode === "detallado" ? "rot" : ""} />
          </button>
          {#if (mode === "detallado" || expanded === p.provider) && p.windows.length}
            <div class="detail">
              {#each p.windows as x}
                <div class="dl"><span class="wl">{x.label}</span>
                  <span class="track"><b style="width:{Math.min(100, x.used)}%;background:{tone(x.used)}"></b></span>
                  <span class="pct">{Math.round(x.used)}%</span></div>
                {#if x.resets_at}<p class="dim3 small rs">se reinicia en {untilReset(x.resets_at)}</p>{/if}
              {/each}
            </div>
          {/if}
        {/each}
        {#if !data.length}<p class="dim3 small empty">{loading ? "Consultando…" : "No encontré planes. Inicia sesión en Claude, Codex o Antigravity."}</p>{/if}
      </div>
      <div class="foot dim3 small">Se consulta cada 15 min con tus sesiones ya iniciadas. Nada sale de tu compu salvo a las APIs oficiales.</div>
    </div>
  {/if}
</div>

<style>
  .usage { position: relative; display: flex; align-items: center; gap: 2px; width: 100%; }
  .line { flex: 1; min-width: 0; overflow: hidden; display: flex; align-items: center; gap: 10px; height: 22px; padding: 0 8px; border-radius: 6px; font-size: 11px; color: var(--text2); }
  .line:hover { background: var(--s2); }
  .item { display: flex; align-items: center; gap: 4px; white-space: nowrap; flex: none; }
  .nowrap { white-space: nowrap; }
  .item b { font-weight: 700; font-variant-numeric: tabular-nums; }
  .refresh { width: 22px; height: 22px; border-radius: 6px; display: flex; align-items: center; justify-content: center; color: var(--text3); }
  .refresh:hover { background: var(--s2); color: var(--text); }
  .spin :global(svg) { animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .scrim { position: fixed; inset: 0; z-index: 14; }
  .pop { position: absolute; left: 0; bottom: 30px; z-index: 15; width: 420px; padding: 12px; box-shadow: 0 20px 50px #0009; }
  .ph { display: flex; align-items: center; gap: 6px; padding: 2px 4px 10px; }
  .ph b { font-size: 13px; }
  .rf { width: 24px; height: 24px; border-radius: 7px; display: flex; align-items: center; justify-content: center; color: var(--text3); }
  .rf:hover { background: var(--s3); color: var(--text); }
  .grow { flex: 1; }
  .small { font-size: 11px; }
  .seg { display: grid; grid-template-columns: 1fr 1fr; background: var(--s2); border-radius: 9px; padding: 3px; margin-bottom: 8px; }
  .seg button { padding: 5px; border-radius: 7px; font-weight: 600; font-size: 12px; color: var(--text2); }
  .seg button.on { background: var(--s3); color: var(--text); box-shadow: 0 1px 2px #0004; }
  .rows { display: flex; flex-direction: column; }
  .row { display: flex; align-items: center; gap: 10px; padding: 10px 6px; border-radius: 9px; text-align: left; font-size: 12.5px; }
  .row:hover { background: var(--s2); }
  .lg { width: 26px; height: 26px; border-radius: 8px; background: var(--s3); display: flex; align-items: center; justify-content: center; flex: none; }
  .nm { flex: 1; font-weight: 700; }
  .track { width: 56px; height: 5px; border-radius: 3px; background: var(--s3); overflow: hidden; display: block; flex: none; }
  .track b { display: block; height: 100%; border-radius: 3px; }
  .pct { width: 36px; text-align: right; font-weight: 700; font-variant-numeric: tabular-nums; }
  .row :global(.rot) { transform: rotate(90deg); }
  .row :global(svg) { color: var(--text3); transition: transform .15s; }
  .detail { padding: 0 6px 8px 42px; }
  .dl { display: flex; align-items: center; gap: 10px; margin-top: 6px; font-size: 12px; }
  .wl { flex: 1; color: var(--text2); }
  .rs { margin: 1px 0 0; }
  .empty { padding: 14px 6px; }
  .foot { border-top: 1px solid var(--line); margin-top: 6px; padding: 9px 6px 2px; line-height: 1.4; }
</style>
