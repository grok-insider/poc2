<script lang="ts">
  import { invoke } from './tauri';
  import type { Goal, Item, Recipe, RecipeSummary } from './types';

  type Props = {
    /** Current item — used as the source for 'Save current as recipe'. */
    item: Item;
    /** Current goal — same. */
    goal: Goal;
    /** Loading a recipe overwrites these via callback. */
    onLoadRecipe: (item: Item, goal: Goal) => void;
  };

  let { item, goal, onLoadRecipe }: Props = $props();

  let recipes = $state<RecipeSummary[]>([]);
  let loading = $state(false);
  let loaded = $state(false);
  let error = $state<string | null>(null);
  let newName = $state('');
  let newDescription = $state('');
  let lastCopied = $state<string | null>(null);

  async function refreshList() {
    loading = true;
    error = null;
    try {
      recipes = await invoke<RecipeSummary[]>('list_recipes');
      loaded = true;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  $effect.pre(() => {
    if (!loaded && !loading) void refreshList();
  });

  function recipeFromCurrent(): Recipe {
    return {
      name: newName.trim(),
      description: newDescription.trim(),
      item_json: JSON.stringify(item),
      goal_json: JSON.stringify(goal),
      created_at: String(Math.floor(Date.now() / 1000)),
    };
  }

  async function saveCurrent() {
    const r = recipeFromCurrent();
    if (!r.name) {
      error = 'Recipe name cannot be empty.';
      return;
    }
    try {
      await invoke('save_recipe', { recipe: r });
      newName = '';
      newDescription = '';
      await refreshList();
    } catch (e) {
      error = String(e);
    }
  }

  async function loadByName(name: string) {
    error = null;
    try {
      const r = await invoke<Recipe>('load_recipe', { name });
      const loadedItem = JSON.parse(r.item_json) as Item;
      const loadedGoal = JSON.parse(r.goal_json) as Goal;
      onLoadRecipe(loadedItem, loadedGoal);
    } catch (e) {
      error = String(e);
    }
  }

  async function deleteByName(name: string) {
    if (!confirm(`Delete recipe "${name}"?`)) return;
    try {
      await invoke('delete_recipe', { name });
      await refreshList();
    } catch (e) {
      error = String(e);
    }
  }

  async function copyAsToml(name: string) {
    try {
      const r = await invoke<Recipe>('load_recipe', { name });
      const toml = await invoke<string>('export_recipe_toml', { recipe: r });
      await navigator.clipboard.writeText(toml);
      lastCopied = name;
      setTimeout(() => {
        if (lastCopied === name) lastCopied = null;
      }, 2000);
    } catch (e) {
      error = String(e);
    }
  }
</script>

<section class="recipes">
  <h2>Recipe library</h2>

  {#if error}
    <pre class="error">{error}</pre>
  {/if}

  <!-- ============== Save current ============== -->
  <div class="block">
    <h3>Save current (item + goal)</h3>
    <div class="save-row">
      <input
        type="text"
        placeholder="recipe-slug (a-z, 0-9, _-)"
        bind:value={newName}
      />
      <input
        type="text"
        placeholder="optional description"
        bind:value={newDescription}
      />
      <button onclick={saveCurrent} disabled={!newName.trim()}>
        Save
      </button>
    </div>
  </div>

  <!-- ============== List ============== -->
  <div class="block">
    <h3>Saved recipes</h3>
    {#if loading}
      <p class="muted">loading…</p>
    {:else if recipes.length === 0}
      <p class="muted">No recipes saved yet.</p>
    {:else}
      <ul class="list">
        {#each recipes as r (r.name)}
          <li>
            <div class="row">
              <span class="name"><code>{r.name}</code></span>
              <div class="actions">
                <button class="action" onclick={() => loadByName(r.name)}>Load</button>
                <button class="action" onclick={() => copyAsToml(r.name)}>
                  {lastCopied === r.name ? 'Copied!' : 'Copy TOML'}
                </button>
                <button class="action danger" onclick={() => deleteByName(r.name)}>
                  Delete
                </button>
              </div>
            </div>
            {#if r.description}
              <p class="description">{r.description}</p>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>

<style>
  .recipes {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.75rem;
  }

  h2 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
  }

  h3 {
    margin: 0 0 0.4rem;
    font-size: 0.85rem;
    color: var(--fg-muted);
  }

  .block {
    margin-bottom: 0.75rem;
  }

  .save-row {
    display: flex;
    gap: 0.4rem;
  }

  .save-row input {
    flex: 1;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--fg);
    padding: 0.35rem 0.5rem;
    border-radius: 2px;
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.8rem;
  }

  .list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .list li {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 0.4rem 0.6rem;
  }

  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.4rem;
  }

  .name {
    color: var(--fg);
    font-size: 0.85rem;
  }

  code {
    color: var(--accent);
  }

  .actions {
    display: flex;
    gap: 0.3rem;
  }

  .action {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--fg-muted);
    border-radius: 2px;
    padding: 0.2rem 0.5rem;
    font-size: 0.7rem;
    cursor: pointer;
  }

  .action:hover {
    border-color: var(--accent);
    color: var(--accent);
  }

  .action.danger:hover {
    border-color: #ff8c8c;
    color: #ff8c8c;
  }

  .description {
    margin: 0.3rem 0 0;
    font-size: 0.75rem;
    color: var(--fg-muted);
  }

  .error {
    background: #2a1010;
    border: 1px solid #5a2222;
    color: #ff8c8c;
    padding: 0.4rem;
    border-radius: 3px;
    font-size: 0.8rem;
  }

  .muted {
    color: var(--fg-muted);
    font-size: 0.85rem;
  }
</style>
