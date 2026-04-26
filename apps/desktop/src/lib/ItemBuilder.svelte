<script lang="ts">
  import type { Item, Rarity } from './types';

  type Props = {
    item: Item;
    onUpdate: (item: Item) => void;
  };

  let { item, onUpdate }: Props = $props();

  function setRarity(r: Rarity) {
    onUpdate({ ...item, rarity: r });
  }

  function setIlvl(v: number) {
    onUpdate({ ...item, ilvl: v });
  }

  function setBase(v: string) {
    onUpdate({ ...item, base: v });
  }

  function toggleCorrupted() {
    onUpdate({ ...item, corrupted: !item.corrupted });
  }

  function toggleSanctified() {
    onUpdate({ ...item, sanctified: !item.sanctified });
  }
</script>

<section class="builder">
  <h2>Item</h2>

  <div class="grid">
    <label>
      Base
      <input
        type="text"
        value={item.base}
        oninput={(e) => setBase((e.currentTarget as HTMLInputElement).value)}
      />
    </label>

    <label>
      ilvl
      <input
        type="number"
        min="1"
        max="100"
        value={item.ilvl}
        oninput={(e) => setIlvl(Number((e.currentTarget as HTMLInputElement).value))}
      />
    </label>
  </div>

  <div class="rarity-row">
    {#each ['normal', 'magic', 'rare', 'unique'] as const as r (r)}
      <button
        class:active={item.rarity === r}
        onclick={() => setRarity(r)}
      >
        {r}
      </button>
    {/each}
  </div>

  <div class="flags">
    <label>
      <input type="checkbox" checked={item.corrupted} onchange={toggleCorrupted} />
      corrupted
    </label>
    <label>
      <input type="checkbox" checked={item.sanctified} onchange={toggleSanctified} />
      sanctified
    </label>
  </div>

  <div class="slots">
    <div class="slot">
      <strong>Prefixes ({item.prefixes.length}/3)</strong>
      <ul>
        {#each item.prefixes as p, i (i)}
          <li>{p.mod_id}{p.is_fractured ? ' (fractured)' : ''}</li>
        {/each}
        {#if item.prefixes.length === 0}
          <li class="muted">none</li>
        {/if}
      </ul>
    </div>
    <div class="slot">
      <strong>Suffixes ({item.suffixes.length}/3)</strong>
      <ul>
        {#each item.suffixes as p, i (i)}
          <li>{p.mod_id}{p.is_fractured ? ' (fractured)' : ''}</li>
        {/each}
        {#if item.suffixes.length === 0}
          <li class="muted">none</li>
        {/if}
      </ul>
    </div>
  </div>

  {#if item.hidden_desecrated}
    <p class="hidden-desecrated">⌛ hidden desecrated mod awaiting reveal</p>
  {/if}

  {#if item.hinekora_lock !== null}
    <p class="lock">🔒 Hinekora's Lock active (seed {item.hinekora_lock})</p>
  {/if}
</section>

<style>
  .grid {
    display: grid;
    grid-template-columns: 1fr 100px;
    gap: 0.75rem;
    margin-bottom: 0.75rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.85rem;
    color: var(--fg-muted);
  }

  input[type='text'],
  input[type='number'] {
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.4rem 0.5rem;
    font: inherit;
  }

  .rarity-row {
    display: flex;
    gap: 0.25rem;
    margin-bottom: 0.75rem;
  }

  .rarity-row button {
    flex: 1;
    background: var(--bg);
    color: var(--fg-muted);
    border: 1px solid var(--border);
    text-transform: capitalize;
    font-weight: 400;
  }

  .rarity-row button.active {
    background: var(--accent);
    color: #1a1100;
    font-weight: 600;
  }

  .flags {
    display: flex;
    gap: 1rem;
    margin-bottom: 0.75rem;
  }

  .flags label {
    flex-direction: row;
    align-items: center;
    gap: 0.4rem;
  }

  .slots {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.75rem;
  }

  .slot ul {
    margin: 0.25rem 0 0;
    padding-left: 1.25rem;
    list-style: disc;
  }

  .muted {
    color: var(--fg-muted);
    list-style: none;
    padding-left: 0;
    font-style: italic;
  }

  .hidden-desecrated,
  .lock {
    margin: 0.5rem 0 0;
    font-size: 0.8rem;
    color: var(--accent);
  }
</style>
