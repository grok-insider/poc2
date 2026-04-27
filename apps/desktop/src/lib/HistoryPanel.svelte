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
    <ul class="timeline">
      {#each entries as e, i (e.id)}
        <li class="timeline-row">
          <span class="badge" aria-hidden="true">✓</span>
          <div class="row-body">
            <header>
              <span class="row-time">{e.timestamp}</span>
              <span class="row-title">Step {entries.length - i}: <em>{e.action_label ?? e.change}</em></span>
              <button class="undo" type="button" onclick={() => onUndo(i)}>Undo</button>
            </header>
            <p class="explanation">{e.explanation}</p>
          </div>
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

  .timeline {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.5rem;
  }

  .timeline-row {
    display: grid;
    grid-template-columns: 28px 1fr;
    gap: 0.6rem;
    align-items: start;
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    background: rgba(0, 0, 0, 0.32);
    padding: 0.5rem 0.6rem;
    position: relative;
  }

  .timeline-row::before {
    content: '';
    position: absolute;
    top: 1.85rem;
    left: 1.4rem;
    bottom: -0.6rem;
    width: 1px;
    background: rgba(197, 143, 61, 0.22);
  }

  .timeline-row:last-child::before {
    display: none;
  }

  .badge {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: #b2ffa2;
    background: rgba(47, 123, 18, 0.42);
    border: 1px solid rgba(114, 255, 88, 0.55);
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.78rem;
    z-index: 1;
  }

  .row-body {
    display: grid;
    gap: 0.2rem;
    min-width: 0;
  }

  header {
    display: flex;
    align-items: baseline;
    gap: 0.55rem;
    flex-wrap: wrap;
  }

  .row-time {
    color: var(--fg-muted);
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.7rem;
  }

  .row-title {
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.88rem;
  }

  .row-title em {
    font-style: normal;
    color: var(--fg-soft);
    font-family: inherit;
  }

  .undo {
    margin-left: auto;
    background: rgba(0, 0, 0, 0.4);
    color: var(--gold);
    border: 1px solid var(--border-gold);
    border-radius: 999px;
    padding: 0.18rem 0.55rem;
    cursor: pointer;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .undo:hover {
    background: rgba(197, 143, 61, 0.18);
    color: var(--gold-bright);
  }

  .explanation {
    margin: 0;
    color: var(--fg-soft);
    font-size: 0.8rem;
    line-height: 1.4;
  }
</style>
