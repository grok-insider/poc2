<script lang="ts">
  import type { HistoryEntry } from './types';

  type Props = {
    entries: HistoryEntry[];
    onUndo: (idx: number) => void;
  };

  let { entries, onUndo }: Props = $props();
</script>

<div class="history">
  {#if entries.length === 0}
    <p class="muted">No outcomes recorded yet. Use "I just used this" to log a step.</p>
  {:else}
    <ul>
      {#each entries as e, i (e.id)}
        <li>
          <header>
            <span class="step">#{entries.length - i}</span>
            <span class="change">{e.change}</span>
            <button class="undo" type="button" onclick={() => onUndo(i)}>Undo</button>
          </header>
          <p class="explanation">{e.explanation}</p>
          <footer>
            <span>{e.timestamp}</span>
          </footer>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .history {
    display: grid;
    gap: 0.4rem;
  }

  .muted {
    color: var(--fg-muted);
    margin: 0;
    font-size: 0.8rem;
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
    padding: 0.4rem 0.55rem;
    background: rgba(0, 0, 0, 0.3);
    display: grid;
    gap: 0.2rem;
  }

  header {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
  }

  .step {
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.85rem;
  }

  .change {
    color: var(--fg);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.7rem;
  }

  .undo {
    margin-left: auto;
    background: rgba(0, 0, 0, 0.4);
    color: var(--gold);
    border: 1px solid var(--border-gold);
    border-radius: 4px;
    padding: 0.2rem 0.5rem;
    cursor: pointer;
    font-size: 0.72rem;
  }

  .undo:hover {
    background: rgba(197, 143, 61, 0.18);
    color: var(--gold-bright);
  }

  .explanation {
    margin: 0;
    color: var(--fg-soft);
    font-size: 0.82rem;
  }

  footer {
    color: var(--fg-muted);
    font-size: 0.72rem;
  }
</style>
