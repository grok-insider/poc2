<script lang="ts">
  import { invoke } from './tauri';
  import type { Item, ParseClipboardResponse } from './types';

  type Props = {
    onItem: (item: Item) => void;
  };

  let { onItem }: Props = $props();

  let text = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  let unresolved = $state<string[]>([]);
  let success = $state<string | null>(null);

  async function importFromText() {
    if (!text.trim()) return;
    busy = true;
    error = null;
    success = null;
    unresolved = [];
    try {
      const r = await invoke<ParseClipboardResponse>('parse_item_text', { text });
      onItem(r.item);
      unresolved = r.unresolved;
      success = `Imported ${r.parsed.rarity} ${r.parsed.item_class} (ilvl ${r.parsed.ilvl}).`;
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  async function importFromClipboard() {
    busy = true;
    error = null;
    success = null;
    unresolved = [];
    try {
      const r = await invoke<ParseClipboardResponse>('read_clipboard_item');
      onItem(r.item);
      unresolved = r.unresolved;
      success = `Imported from clipboard: ${r.parsed.rarity} ${r.parsed.item_class}.`;
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }
</script>

<section class="clipboard">
  <h2>Import item</h2>

  <div class="actions">
    <button onclick={importFromClipboard} disabled={busy}>
      {busy ? 'reading…' : 'Read clipboard'}
    </button>
  </div>

  <details>
    <summary>Or paste item text manually</summary>
    <textarea
      bind:value={text}
      placeholder="Paste PoE2 in-game item text (Ctrl+C in game)"
      rows="6"
    ></textarea>
    <button onclick={importFromText} disabled={busy || !text.trim()}>Parse</button>
  </details>

  {#if success}
    <p class="success">{success}</p>
  {/if}
  {#if error}
    <pre class="error">{error}</pre>
  {/if}
  {#if unresolved.length > 0}
    <details class="unresolved">
      <summary>{unresolved.length} mod line(s) didn't resolve to a known mod</summary>
      <ul>
        {#each unresolved as line, i (i)}
          <li>{line}</li>
        {/each}
      </ul>
      <p class="muted">
        Mods only resolve when a mod registry (data bundle) is loaded; M6+ wires
        the bundle, so unresolved mods are expected on a fresh install.
      </p>
    </details>
  {/if}
</section>

<style>
  .actions {
    margin-bottom: 0.5rem;
  }

  textarea {
    width: 100%;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.5rem;
    font: 0.85rem ui-monospace, 'Fira Code', monospace;
    margin: 0.5rem 0;
    resize: vertical;
  }

  details {
    margin-top: 0.5rem;
  }

  details summary {
    cursor: pointer;
    color: var(--fg-muted);
    font-size: 0.85rem;
  }

  .success {
    margin: 0.5rem 0 0;
    font-size: 0.85rem;
    color: #a6d09a;
  }

  .error {
    margin: 0.5rem 0 0;
    background: #2a1010;
    border-color: #5a2222;
    color: #ff8c8c;
    padding: 0.5rem;
    border-radius: 4px;
    font-size: 0.85rem;
  }

  .unresolved {
    margin-top: 0.5rem;
  }

  .unresolved ul {
    margin: 0.5rem 0;
    padding-left: 1.25rem;
    font-size: 0.85rem;
    color: var(--fg-muted);
  }

  .muted {
    color: var(--fg-muted);
    font-size: 0.8rem;
    margin: 0.5rem 0 0;
  }
</style>
