<script lang="ts">
  // One xterm.js instance for the whole app: switching tasks repaints it from the Rust-side
  // scrollback, so there is no per-task emulator eating RAM (Orca keeps one per pane, twice).
  import { onMount, onDestroy } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { api } from "./api";

  // termId: which pseudo-terminal to show; open(): starts it if needed and returns its scrollback
  let { termId, open, onReady }: { termId: string | null; open: (() => Promise<string>) | null; onReady?: () => void } = $props();

  let el: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;
  let unlisten: UnlistenFn | undefined;
  let ro: ResizeObserver;
  let current: string | null = null;

  // Terminal background follows the theme's --term color (solid, so agents' colored cells blend in)
  function termBg() {
    return getComputedStyle(document.documentElement).getPropertyValue("--term").trim() || "#232326";
  }
  let themeObs: MutationObserver;

  onMount(async () => {
    term = new Terminal({
      fontFamily: '"GeistMono Nerd Font", "SF Mono", "JetBrains Mono", monospace',
      fontSize: 13,
      lineHeight: 1.2,
      cursorBlink: false, // no constant redraw
      scrollback: 3000,
      allowProposedApi: true,
      theme: {
        background: termBg(), foreground: "#d1d1d6", cursor: "#f5f5f7", selectionBackground: "#3a3a4a",
        black: "#1c1c22", red: "#ff6b62", green: "#30d158", yellow: "#ffd60a", blue: "#64a8ff",
        magenta: "#c4a5ff", cyan: "#5ac8fa", white: "#d1d1d6", brightBlack: "#6e6e78",
      },
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.open(el);
    fit.fit();
    term.onData((d) => { if (current) api.write(current, d).catch(() => {}); });
    term.onResize(({ cols, rows }) => { if (current) api.resize(current, cols, rows).catch(() => {}); });
    unlisten = await listen<{ id: string; data: string }>("pty-data", (e) => {
      if (e.payload.id === current) term.write(e.payload.data);
    });
    themeObs = new MutationObserver(() => { term.options.theme = { ...term.options.theme, background: termBg() }; });
    themeObs.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    ro = new ResizeObserver(() => { try { fit.fit(); } catch {} });
    ro.observe(el);
    onReady?.();
  });

  onDestroy(() => { unlisten?.(); ro?.disconnect(); themeObs?.disconnect(); term?.dispose(); });

  function attach(id: string, opener: () => Promise<string>) {
    current = id;
    term.reset();
    opener().then((back) => {
      if (current !== id) return;
      if (back) term.write(back);
      api.resize(id, term.cols, term.rows).catch(() => {});
      term.focus();
    }).catch((e) => term.write(`\r\n\x1b[31m${e}\x1b[0m\r\n`));
  }

  // Switch terminal: clear, replay its scrollback (or start it), sync the size.
  $effect(() => {
    const id = termId;
    const opener = open;
    if (!term || id === current) return;
    if (!id || !opener) { current = id; term.reset(); return; }
    attach(id, opener);
  });

  /** Re-open the current terminal (e.g. after starting or waking its agent). */
  export function reload() { if (current && open) attach(current, open); }

  export function focus() { term?.focus(); }
</script>

<div class="wrap"><div class="term" bind:this={el}></div></div>

<style>
  .wrap { position: absolute; inset: 0; padding: 10px 6px 6px 12px; background: var(--term); }
  .term { width: 100%; height: 100%; }
  :global(.xterm-viewport) { background: transparent !important; }
</style>
