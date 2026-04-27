<script lang="ts">
  import { invoke } from './tauri';
  import { baseIconUrl, loadBaseIconManifest } from './baseIcons';
  import type {
    BaseIconManifest,
    BaseSummary,
    DatabaseEntryDetail,
    DatabaseEntrySummary,
    DatabaseSection,
  } from './types';

  type Mode = 'inspect' | 'pick-base';

  type Props = {
    open: boolean;
    mode?: Mode;
    initialClass?: string | null;
    onClose: () => void;
    onPick?: (base: BaseSummary) => void;
  };

  let { open, mode = 'inspect', initialClass = null, onClose, onPick }: Props = $props();

  let section = $state<DatabaseSection>('bases');
  let search = $state('');
  let entries = $state<DatabaseEntrySummary[]>([]);
  let selectedId = $state<string | null>(null);
  let detail = $state<DatabaseEntryDetail | null>(null);
  let manifest = $state<BaseIconManifest | null>(null);
  let loading = $state(false);
  let detailLoading = $state(false);
  let error = $state<string | null>(null);
  let classFilter = $state<string | null>(null);

  const visibleEntries = $derived(
    entries.filter((entry) => !classFilter || entry.kind === classFilter),
  );
  const classCounts = $derived(buildCounts(entries));
  const selected = $derived(visibleEntries.find((entry) => entry.id === selectedId) ?? visibleEntries[0] ?? null);

  $effect(() => {
    if (!open) return;
    section = mode === 'pick-base' ? 'bases' : section;
    classFilter = initialClass;
    void loadBaseIconManifest().then((m) => (manifest = m));
  });

  $effect(() => {
    if (!open) return;
    void section;
    void search;
    void loadEntries();
  });

  $effect(() => {
    if (!open || !selected) return;
    selectedId = selected.id;
    void loadDetail(selected);
  });

  async function loadEntries() {
    loading = true;
    error = null;
    try {
      const result = await invoke<DatabaseEntrySummary[]>('list_database_entries', {
        args: { section, search },
      });
      entries = result;
      if (!result.some((entry) => entry.id === selectedId)) {
        selectedId = result[0]?.id ?? null;
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function loadDetail(entry: DatabaseEntrySummary) {
    detailLoading = true;
    try {
      detail = await invoke<DatabaseEntryDetail>('database_entry_detail', {
        args: { section: entry.section, id: entry.id },
      });
    } catch (e) {
      error = String(e);
      detail = null;
    } finally {
      detailLoading = false;
    }
  }

  function buildCounts(list: DatabaseEntrySummary[]): { kind: string; label: string; count: number }[] {
    const counts = new Map<string, { label: string; count: number }>();
    for (const entry of list) {
      const current = counts.get(entry.kind) ?? { label: entry.category, count: 0 };
      current.count += 1;
      counts.set(entry.kind, current);
    }
    return [...counts.entries()]
      .map(([kind, value]) => ({ kind, label: value.label, count: value.count }))
      .sort((a, b) => b.count - a.count || a.label.localeCompare(b.label));
  }

  function select(entry: DatabaseEntrySummary) {
    selectedId = entry.id;
  }

  function useBase(entry: DatabaseEntrySummary) {
    if (!entry.base || !onPick) return;
    onPick(entry.base);
    onClose();
  }

  function entryIcon(entry: DatabaseEntrySummary): string | null {
    if (entry.base) return baseIconUrl(manifest, entry.base.id);
    return entry.icon_url ?? null;
  }
</script>

{#if open}
  <button class="scrim" type="button" aria-label="Close database" onclick={onClose}></button>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Item Database">
    <header>
      <div>
        <span class="kicker">Item Database</span>
        <h2>{mode === 'pick-base' ? 'Pick a base' : 'Inspect items'}</h2>
      </div>
      <button class="ghost compact" onclick={onClose}>Close</button>
    </header>

    <div class="toolbar">
      <div class="tabs">
        <button class:active={section === 'bases'} onclick={() => ((section = 'bases'), (classFilter = null))}>Base Items</button>
        {#if mode === 'inspect'}
          <button class:active={section === 'materials'} onclick={() => ((section = 'materials'), (classFilter = null))}>Crafting Materials</button>
        {/if}
      </div>
      <input type="search" placeholder="Search name, class, tag, effect…" bind:value={search} />
      <div class="chips">
        <button class="chip" class:active={classFilter === null} onclick={() => (classFilter = null)}>
          All <em>{entries.length}</em>
        </button>
        {#each classCounts as c (c.kind)}
          <button class="chip" class:active={classFilter === c.kind} onclick={() => (classFilter = c.kind)}>
            {c.label} <em>{c.count}</em>
          </button>
        {/each}
      </div>
    </div>

    <div class="content">
      <aside class="list">
        {#if loading}
          <p class="muted">Loading database…</p>
        {:else if error}
          <p class="error">{error}</p>
        {:else if visibleEntries.length === 0}
          <p class="muted">No entries match that filter.</p>
        {:else}
          {#each visibleEntries as entry (entry.id)}
            <button class="row" class:active={selected?.id === entry.id} onclick={() => select(entry)}>
              <div class="thumb">
                {#if entryIcon(entry)}
                  <img src={entryIcon(entry) ?? ''} alt="" />
                {:else}
                  <span>{entry.name.slice(0, 2)}</span>
                {/if}
              </div>
              <div class="row-meta">
                <strong>{entry.name}</strong>
                <small>{entry.category}</small>
                {#if entry.base}
                  <small>ilvl {entry.base.drop_level} · {entry.base.attribute_pool}</small>
                {/if}
              </div>
            </button>
          {/each}
        {/if}
      </aside>

      <section class="detail">
        {#if detailLoading && !detail}
          <p class="muted">Loading details…</p>
        {:else if selected && detail}
          <article class="item-card">
            <div class="card-main">
              <div class="card-title">{detail.summary.name}</div>
              <div class="class-line">{detail.summary.category}</div>

              {#if detail.base}
                {#each detail.base.derived_stats as stat (stat.label)}
                  <div class="stat" title={stat.help ?? ''}>
                    <span class="dotted">{stat.label}</span>: <strong>{stat.value}</strong>
                  </div>
                {/each}
                {#each detail.base.granted_effects as effect (effect.label + effect.value)}
                  <div class="stat granted" title={effect.help ?? ''}>
                    <span class="dotted">{effect.label}</span>: <strong>{effect.value}</strong>
                  </div>
                {/each}
                <div class="requires">Requires: {detail.base.requirements.join(', ')}</div>
                {#if detail.base.class_notes.length > 0}
                  <div class="class-notes">
                    {#each detail.base.class_notes as note (note)}
                      <p>{note}</p>
                    {/each}
                  </div>
                {/if}
                <dl>
                  <dt>Drop Level</dt><dd>{detail.base.drop_level}</dd>
                  <dt>Inventory</dt><dd>{detail.base.inventory_width}x{detail.base.inventory_height}</dd>
                  <dt>Type</dt><dd>{detail.base.metadata_type}</dd>
                </dl>
              {:else if detail.material}
                <p class="material-description">{detail.material.description}</p>
                {#if detail.material.applies_to.length > 0}
                  <p class="requires">Applies to: {detail.material.applies_to.join(', ')}</p>
                {/if}
                {#if detail.material.raw_fields.length > 0}
                  <dl>
                    {#each detail.material.raw_fields as field (field.label)}
                      <dt>{field.label}</dt><dd>{field.value}</dd>
                    {/each}
                  </dl>
                {/if}
              {/if}

              <div class="tags">
                {#each detail.summary.tags as tag (tag)}
                  <span>{tag}</span>
                {/each}
              </div>
            </div>
            <div class="art-large">
              {#if entryIcon(detail.summary)}
                <img src={entryIcon(detail.summary) ?? ''} alt="" />
              {:else}
                <span>{detail.summary.name.slice(0, 2)}</span>
              {/if}
            </div>
          </article>

          {#if mode === 'pick-base' && detail?.summary.base}
            <button class="use" onclick={() => detail && useBase(detail.summary)}>Use as base</button>
          {/if}
        {:else}
          <p class="muted">Select an entry to inspect it.</p>
        {/if}
      </section>
    </div>

    <footer>
      <span>{visibleEntries.length} shown</span>
      <button class="ghost" onclick={onClose}>Close</button>
    </footer>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.65);
    border: 0;
    z-index: 50;
  }

  .dialog {
    position: fixed;
    inset: 3vh 3vw;
    display: grid;
    grid-template-rows: auto auto 1fr auto;
    background: linear-gradient(180deg, rgba(14, 17, 18, 0.99), rgba(2, 4, 5, 0.99));
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    z-index: 51;
    overflow: hidden;
    box-shadow: 0 30px 90px rgba(0, 0, 0, 0.75);
  }

  header, footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.7rem 0.85rem;
    border-color: var(--border-strong);
    background: rgba(18, 11, 4, 0.9);
  }

  header { border-bottom: 1px solid var(--border-strong); }
  footer { border-top: 1px solid var(--border-strong); color: var(--fg-muted); }

  h2 {
    margin: 0.1rem 0 0;
    color: var(--gold-bright);
    font: 1.15rem Georgia, 'Times New Roman', serif;
  }

  .kicker {
    color: var(--gold);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font: 0.72rem Georgia, 'Times New Roman', serif;
  }

  .toolbar {
    display: grid;
    gap: 0.5rem;
    padding: 0.65rem 0.85rem;
    border-bottom: 1px solid var(--border-strong);
    background: rgba(0, 0, 0, 0.35);
  }

  .tabs, .chips { display: flex; flex-wrap: wrap; gap: 0.35rem; }

  .tabs button, .chip, .ghost, .use {
    background: rgba(0, 0, 0, 0.35);
    color: var(--fg-soft);
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.38rem 0.65rem;
    cursor: pointer;
  }

  .tabs button.active, .chip.active {
    color: var(--gold-bright);
    border-color: var(--gold);
    background: linear-gradient(90deg, rgba(197, 143, 61, 0.32), rgba(20, 14, 5, 0.7));
  }

  .chip { border-radius: 999px; font-size: 0.78rem; padding: 0.25rem 0.55rem; }
  .chip em { color: var(--fg-muted); font-style: normal; margin-left: 0.25rem; }

  input {
    background: rgba(0, 0, 0, 0.55);
    color: var(--fg);
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.45rem 0.6rem;
  }

  .content {
    display: grid;
    grid-template-columns: minmax(260px, 34%) 1fr;
    min-height: 0;
  }

  .list {
    overflow: auto;
    padding: 0.65rem;
    border-right: 1px solid var(--border-strong);
  }

  .row {
    width: 100%;
    display: grid;
    grid-template-columns: 54px 1fr;
    gap: 0.6rem;
    align-items: center;
    margin-bottom: 0.45rem;
    padding: 0.4rem;
    background: rgba(0, 0, 0, 0.28);
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
  }

  .row.active, .row:hover { border-color: var(--gold); background: rgba(197, 143, 61, 0.08); }

  .thumb, .art-large {
    display: grid;
    place-items: center;
    border: 1px solid rgba(197, 143, 61, 0.35);
    background: radial-gradient(circle, rgba(31, 54, 61, 0.55), rgba(0, 0, 0, 0.75) 80%);
    overflow: hidden;
  }

  .thumb { width: 50px; height: 50px; border-radius: 4px; }
  .thumb img { max-width: 44px; max-height: 44px; object-fit: contain; }
  .thumb span, .art-large span { color: #00c8ff; font-weight: 700; font-family: Georgia, 'Times New Roman', serif; }

  .row-meta { display: grid; gap: 0.12rem; min-width: 0; }
  .row-meta strong { color: var(--gold-bright); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-meta small { color: var(--fg-muted); }

  .detail { overflow: auto; padding: 1rem; }

  .item-card {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 210px;
    gap: 1rem;
    min-height: 320px;
    padding: 0.55rem;
    border: 2px solid #303030;
    background: #020202;
    box-shadow: inset 0 0 0 1px #111;
  }

  .card-title {
    margin-bottom: 0.65rem;
    padding: 0.25rem;
    border: 1px solid #555;
    border-radius: 999px;
    color: #e8e0d0;
    text-align: center;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font: 1.1rem Georgia, 'Times New Roman', serif;
    background: linear-gradient(180deg, #242424, #090909);
  }

  .class-line, .stat, .requires, .material-description {
    color: #b9b0a6;
    text-align: center;
    font-size: 0.95rem;
    line-height: 1.45;
  }

  .stat strong, .requires { color: #f0e5c0; }
  .stat.granted strong { color: #9aa7ff; font-variant: small-caps; }
  .dotted { border-bottom: 1px dotted #9a8a70; cursor: help; }
  .material-description { max-width: 58ch; margin: 1rem auto; color: #d8c68f; }

  .class-notes {
    max-width: 58ch;
    margin: 0.75rem auto;
    border: 1px solid rgba(197, 143, 61, 0.22);
    border-radius: 4px;
    padding: 0.65rem 0.8rem;
    background: rgba(0, 0, 0, 0.35);
    color: #d8c68f;
    text-align: center;
    font-size: 0.85rem;
    line-height: 1.45;
  }

  .class-notes p { margin: 0.25rem 0; }

  .art-large { min-height: 280px; }
  .art-large img { max-width: 190px; max-height: 280px; object-fit: contain; }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.3rem 0.7rem;
    margin: 1rem 0;
    color: var(--fg-muted);
    font-size: 0.82rem;
  }

  dt { color: var(--gold); }
  dd { margin: 0; overflow-wrap: anywhere; }

  .tags { display: flex; flex-wrap: wrap; gap: 0.3rem; margin-top: 1rem; }
  .tags span { border: 1px solid var(--border-strong); border-radius: 999px; padding: 0.15rem 0.45rem; color: var(--fg-muted); font-size: 0.72rem; }

  .use { margin-top: 0.75rem; color: var(--gold-bright); border-color: var(--gold); }
  .ghost.compact { padding: 0.3rem 0.55rem; }
  .muted { color: var(--fg-muted); margin: 0; }
  .error { color: #ff8c8c; margin: 0; }

  @media (max-width: 760px) {
    .dialog { inset: 1vh 1vw; }
    .content { grid-template-columns: 1fr; }
    .list { max-height: 34vh; border-right: 0; border-bottom: 1px solid var(--border-strong); }
    .item-card { grid-template-columns: 1fr; }
    .art-large { min-height: 160px; }
  }
</style>
