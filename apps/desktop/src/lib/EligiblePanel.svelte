<script lang="ts">
  import { invoke } from './tauri';
  import type { EligibleModView, EligibleModsResponse, Item } from './types';

  type Props = {
    item: Item;
    targetConcepts?: string[];
  };

  let { item, targetConcepts = [] }: Props = $props();

  let response = $state<EligibleModsResponse | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  /** Phase D.2 — match the OutcomeDialog's source chips so the user
   * can compare currency vs essence vs desecrated vs Vaal pools
   * without opening the dialog. */
  type RollSource = 'all' | 'currency' | 'essence' | 'desecrated' | 'vaal';
  let rollSource = $state<RollSource>('all');

  $effect(() => {
    void item;
    loading = true;
    error = null;
    invoke<EligibleModsResponse>('eligible_mods', {
      args: { item, affix: 'either', min_required_level: 0 },
    })
      .then((r) => {
        response = r;
      })
      .catch((e) => {
        error = String(e);
      })
      .finally(() => {
        loading = false;
      });
  });

  const grouped = $derived(group(response?.mods ?? [], rollSource));

  function passesSource(m: EligibleModView, source: RollSource): boolean {
    if (source === 'currency') {
      return m.kind === 'explicit' && !m.is_essence_only && !m.is_desecrated_only;
    }
    if (source === 'essence') return m.is_essence_only;
    if (source === 'desecrated') return m.kind === 'desecrated' || m.is_desecrated_only;
    if (source === 'vaal') return m.kind === 'corrupted';
    return true;
  }

  function group(mods: EligibleModView[], source: RollSource) {
    const buckets = new Map<string, EligibleModView[]>();
    for (const m of mods) {
      // Phase D.2 — apply the chip filter before bucketing.
      if (!passesSource(m, source)) continue;
      // The "all" view still requires the mod to be eligible now;
      // other source views (Vaal, desecrated, essence) include
      // *all* matching mods regardless of slot occupancy because
      // they roll via different paths.
      if (source === 'all' && !m.eligible_now) continue;
      const list = buckets.get(m.mod_group) ?? [];
      list.push(m);
      buckets.set(m.mod_group, list);
    }
    const out: { group: string; best: EligibleModView; count: number; satisfies: boolean }[] = [];
    for (const [g, list] of buckets) {
      list.sort((a, b) => a.tier_index - b.tier_index);
      const best = list[0];
      const satisfies = best.concepts.some((c) => targetConcepts.includes(c));
      out.push({ group: g, best, count: list.length, satisfies });
    }
    out.sort((a, b) => Number(b.satisfies) - Number(a.satisfies) || a.group.localeCompare(b.group));
    return out;
  }
</script>

<div class="eligible">
  <!-- Phase D.2 — Source filter chips, mirroring the OutcomeDialog. -->
  <div class="chip-row">
    {#each [
      { v: 'all', label: 'All' },
      { v: 'currency', label: 'Currency' },
      { v: 'essence', label: 'Essence' },
      { v: 'desecrated', label: 'Desecrated' },
      { v: 'vaal', label: 'Vaal' },
    ] as opt (opt.v)}
      <button
        type="button"
        class="chip"
        class:active={rollSource === opt.v}
        onclick={() => (rollSource = opt.v as RollSource)}
      >
        {opt.label}
      </button>
    {/each}
  </div>

  {#if loading}
    <p class="muted">Loading eligible mods…</p>
  {:else if error}
    <p class="error">{error}</p>
  {:else if !response || !response.data_available}
    <p class="muted">
      No mod data bundled for {response?.item_class ?? item.base}.
    </p>
  {:else if grouped.length === 0}
    <p class="muted">No mods in this source for the current item.</p>
  {:else}
    <p class="meta">{grouped.length} mod groups in {rollSource === 'all' ? 'eligible' : rollSource} pool</p>
    <ul>
      {#each grouped as g (g.group)}
        <li class:satisfies={g.satisfies}>
          <header>
            <span class="tier">T{g.best.tier_index}</span>
            <span class="name">{g.best.name ?? g.group}</span>
            <span class="affix">{g.best.affix_type}</span>
          </header>
          {#if g.best.text_template}
            <p class="tpl">{g.best.text_template}</p>
          {/if}
          <footer>
            <span class="count">
              {g.count} {rollSource === 'all' ? 'eligible' : 'pool'} tier{g.count === 1 ? '' : 's'}
            </span>
            <span class="weight">share {(g.best.weight_share * 100).toFixed(1)}%</span>
          </footer>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .eligible {
    display: grid;
    gap: 0.4rem;
  }

  .meta,
  .muted {
    color: var(--fg-muted);
    margin: 0;
    font-size: 0.78rem;
  }

  .error {
    color: #ff8c8c;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.35rem;
  }

  li {
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.4rem 0.5rem;
    background: rgba(0, 0, 0, 0.3);
    display: grid;
    gap: 0.2rem;
  }

  li.satisfies {
    border-color: rgba(114, 255, 88, 0.55);
    background: rgba(20, 35, 18, 0.45);
  }

  header {
    display: flex;
    align-items: baseline;
    gap: 0.4rem;
  }

  .tier {
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
  }

  .name {
    color: var(--fg);
    font-weight: 600;
    font-size: 0.85rem;
  }

  .affix {
    color: var(--fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.7rem;
    margin-left: auto;
  }

  .tpl {
    margin: 0;
    color: #00c8ff;
    font-size: 0.78rem;
    white-space: pre-wrap;
  }

  footer {
    display: flex;
    justify-content: space-between;
    color: var(--fg-muted);
    font-size: 0.72rem;
  }

  .weight {
    color: #72ff58;
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  /* Phase D.2 — chip row */
  .chip-row {
    display: flex;
    gap: 0.3rem;
    flex-wrap: wrap;
  }

  .chip {
    background: rgba(0, 0, 0, 0.4);
    color: var(--fg-soft);
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    padding: 0.2rem 0.65rem;
    font-size: 0.74rem;
    cursor: pointer;
    transition: background 0.1s, color 0.1s, border-color 0.1s;
  }

  .chip:hover {
    border-color: var(--border-gold);
    color: var(--fg);
  }

  .chip.active {
    background: linear-gradient(180deg, rgba(220, 165, 70, 0.85), rgba(150, 105, 30, 0.85));
    color: #1a1100;
    border-color: rgba(255, 211, 122, 0.85);
    font-weight: 600;
  }
</style>
