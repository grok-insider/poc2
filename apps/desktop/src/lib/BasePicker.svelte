<script lang="ts">
  import { invoke } from './tauri';
  import { baseIconUrl, loadBaseIconManifest } from './baseIcons';
  import type { BaseIconManifest, BaseSummary } from './types';

  type Props = {
    open: boolean;
    initialClass?: string | null;
    onClose: () => void;
    onPick: (base: BaseSummary) => void;
  };

  let { open, initialClass = null, onClose, onPick }: Props = $props();

  let bases = $state<BaseSummary[]>([]);
  let manifest = $state<BaseIconManifest | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let search = $state('');
  let classFilter = $state<string | null>(null);
  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);

  // Tile layout (matches CSS): each row is ~110px tall, with 4 columns at >=1180px.
  const ROW_HEIGHT = 96;
  const ITEM_PADDING = 6;

  let containerHeight = $state(360);
  let columns = $state(4);

  $effect(() => {
    void open;
    if (!open) return;
    if (bases.length > 0) return;
    void load();
  });

  $effect(() => {
    if (!open) return;
    void loadBaseIconManifest().then((m) => {
      manifest = m;
    });
  });

  $effect(() => {
    if (!viewport) return;
    const ro = new ResizeObserver(() => {
      if (!viewport) return;
      containerHeight = viewport.clientHeight;
      const w = viewport.clientWidth;
      columns = w >= 1180 ? 4 : w >= 720 ? 3 : 2;
    });
    ro.observe(viewport);
    return () => ro.disconnect();
  });

  $effect(() => {
    classFilter = initialClass ?? null;
  });

  async function load() {
    loading = true;
    error = null;
    try {
      const r = await invoke<BaseSummary[]>('list_bases', {
        args: { class_pascal: null, include_legacy: false },
      });
      bases = r;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  const classCounts = $derived(buildClassCounts(bases));
  const filtered = $derived(filterBases(bases, classFilter, search));
  const rows = $derived(Math.ceil(filtered.length / columns));
  const totalHeight = $derived(rows * ROW_HEIGHT + ITEM_PADDING * 2);
  const startRow = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - 2));
  const endRow = $derived(
    Math.min(rows, Math.ceil((scrollTop + containerHeight) / ROW_HEIGHT) + 2),
  );
  const visible = $derived(
    filtered.slice(startRow * columns, endRow * columns).map((entry, i) => ({
      base: entry,
      idx: startRow * columns + i,
    })),
  );

  function buildClassCounts(list: BaseSummary[]): { class_pascal: string; count: number }[] {
    const counts = new Map<string, number>();
    for (const b of list) counts.set(b.class_pascal, (counts.get(b.class_pascal) ?? 0) + 1);
    return [...counts.entries()]
      .map(([class_pascal, count]) => ({ class_pascal, count }))
      .sort((a, b) => b.count - a.count);
  }

  function filterBases(
    list: BaseSummary[],
    cls: string | null,
    term: string,
  ): BaseSummary[] {
    const q = term.trim().toLowerCase();
    return list.filter((b) => {
      if (cls && b.class_pascal !== cls) return false;
      if (!q) return true;
      return b.name.toLowerCase().includes(q);
    });
  }

  function onScroll(e: Event) {
    scrollTop = (e.target as HTMLDivElement).scrollTop;
  }

  function pick(b: BaseSummary) {
    onPick(b);
    onClose();
  }
</script>

{#if open}
  <button class="scrim" type="button" aria-label="Close picker" onclick={onClose}></button>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Pick base">
    <header>
      <div>
        <span class="kicker">Item Database</span>
        <h2>Pick a base</h2>
      </div>
      <button class="ghost compact" onclick={onClose}>Close</button>
    </header>

    <div class="filters">
      <input
        type="search"
        placeholder="Search by name…"
        bind:value={search}
      />
      <div class="chips">
        <button
          type="button"
          class="chip"
          class:active={classFilter === null}
          onclick={() => (classFilter = null)}
        >
          All <em>{bases.length}</em>
        </button>
        {#each classCounts as c (c.class_pascal)}
          <button
            type="button"
            class="chip"
            class:active={classFilter === c.class_pascal}
            onclick={() => (classFilter = c.class_pascal)}
          >
            {c.class_pascal} <em>{c.count}</em>
          </button>
        {/each}
      </div>
    </div>

    <div
      class="viewport"
      bind:this={viewport}
      onscroll={onScroll}
    >
      {#if loading}
        <p class="muted">Loading bases…</p>
      {:else if error}
        <p class="error">{error}</p>
      {:else if filtered.length === 0}
        <p class="muted">No bases match that filter.</p>
      {:else}
        <div class="virtual" style:height="{totalHeight}px">
          <div
            class="grid"
            style:transform="translateY({startRow * ROW_HEIGHT}px)"
            style:--cols={columns}
          >
            {#each visible as v (v.base.id)}
              <button class="tile" type="button" onclick={() => pick(v.base)}>
                <div class="art">
                  {#if baseIconUrl(manifest, v.base.id)}
                    <img src={baseIconUrl(manifest, v.base.id) ?? ''} alt="" />
                  {:else}
                    <span>{v.base.name.slice(0, 2)}</span>
                  {/if}
                </div>
                <div class="meta">
                  <strong>{v.base.name}</strong>
                  <span class="line">
                    <span class="cls">{v.base.class_pascal}</span>
                    <span class="lvl">ilvl {v.base.drop_level}</span>
                    <span class="pool">{v.base.attribute_pool}</span>
                  </span>
                </div>
              </button>
            {/each}
          </div>
        </div>
      {/if}
    </div>

    <footer class="actions">
      <span class="meta-count">{filtered.length} of {bases.length} bases</span>
      <button class="ghost" onclick={onClose}>Cancel</button>
    </footer>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    border: 0;
    cursor: pointer;
    z-index: 50;
  }

  .dialog {
    position: fixed;
    top: 5vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(960px, 94vw);
    height: 88vh;
    display: grid;
    grid-template-rows: auto auto 1fr auto;
    background: linear-gradient(180deg, rgba(15, 19, 22, 0.98), rgba(5, 8, 11, 0.98));
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    z-index: 51;
    overflow: hidden;
    box-shadow: 0 30px 80px rgba(0, 0, 0, 0.7);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 0.85rem;
    border-bottom: 1px solid var(--border-strong);
    background: rgba(20, 13, 4, 0.85);
  }

  header h2 {
    margin: 0.1rem 0 0;
    font-family: Georgia, 'Times New Roman', serif;
    color: var(--gold-bright);
    font-size: 1.1rem;
  }

  .kicker {
    color: var(--gold);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.75rem;
  }

  .filters {
    display: grid;
    gap: 0.45rem;
    padding: 0.55rem 0.75rem;
    border-bottom: 1px solid var(--border-strong);
    background: rgba(0, 0, 0, 0.3);
  }

  .filters input {
    background: rgba(0, 0, 0, 0.5);
    color: var(--fg);
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.4rem 0.6rem;
    font-size: 0.9rem;
  }

  .chips {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
  }

  .chip {
    background: rgba(0, 0, 0, 0.4);
    color: var(--fg-soft);
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    padding: 0.25rem 0.55rem;
    font-size: 0.78rem;
    cursor: pointer;
  }

  .chip em {
    font-style: normal;
    color: var(--fg-muted);
    margin-left: 0.3rem;
  }

  .chip.active {
    color: var(--gold-bright);
    border-color: var(--gold);
    background: linear-gradient(90deg, rgba(197, 143, 61, 0.32), rgba(20, 14, 5, 0.7));
  }

  .viewport {
    overflow-y: auto;
    overflow-x: hidden;
    padding: 0.5rem 0.75rem;
    position: relative;
  }

  .virtual {
    position: relative;
    width: 100%;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(var(--cols, 4), minmax(0, 1fr));
    gap: 0.45rem;
    will-change: transform;
  }

  .tile {
    display: grid;
    grid-template-columns: 64px 1fr;
    gap: 0.55rem;
    align-items: center;
    padding: 0.45rem;
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
    height: 88px;
  }

  .tile:hover {
    border-color: var(--border-gold);
    background: rgba(197, 143, 61, 0.08);
  }

  .art {
    width: 64px;
    height: 64px;
    border: 1px solid rgba(197, 143, 61, 0.35);
    border-radius: 4px;
    background:
      radial-gradient(circle, rgba(31, 54, 61, 0.65), rgba(0, 0, 0, 0.6) 80%);
    display: grid;
    place-items: center;
    overflow: hidden;
  }

  .art img {
    max-width: 56px;
    max-height: 56px;
    object-fit: contain;
  }

  .art span {
    color: #00c8ff;
    font-weight: 700;
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 1rem;
  }

  .meta {
    display: grid;
    gap: 0.2rem;
    min-width: 0;
  }

  .meta strong {
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.95rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .line {
    display: flex;
    gap: 0.5rem;
    color: var(--fg-muted);
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .cls {
    color: var(--gold);
  }

  .actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.55rem 0.75rem;
    border-top: 1px solid var(--border-strong);
    background: rgba(0, 0, 0, 0.4);
  }

  .meta-count {
    color: var(--fg-muted);
    font-size: 0.78rem;
  }

  .ghost {
    background: rgba(0, 0, 0, 0.35);
    color: var(--gold);
    border: 1px solid var(--border-gold);
    border-radius: 4px;
    padding: 0.4rem 0.7rem;
    cursor: pointer;
    font-size: 0.82rem;
  }

  .ghost.compact {
    padding: 0.3rem 0.55rem;
    font-size: 0.78rem;
  }

  .muted {
    color: var(--fg-muted);
    margin: 0;
  }

  .error {
    color: #ff8c8c;
    margin: 0;
  }
</style>
