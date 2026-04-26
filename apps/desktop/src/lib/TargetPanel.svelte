<script lang="ts">
  import type { Goal, TargetSpec, DivEquiv } from './types';

  /// Default concept list for the autocomplete datalist. The
  /// SettingsPanel (Phase B.3) will replace this with the bundle's
  /// full concept set after a reload.
  const DEFAULT_CONCEPTS = [
    'EnergyShield',
    'Life',
    'Mana',
    'Strength',
    'Dexterity',
    'Intelligence',
    'AllAttributes',
    'FireResistance',
    'ColdResistance',
    'LightningResistance',
    'ChaosResistance',
    'AllResistances',
    'Armour',
    'Evasion',
    'AttackDamage',
    'SpellDamage',
    'CastSpeed',
    'CriticalStrikeChance',
    'CriticalStrikeMultiplier',
    'AttackSpeed',
    'MovementSpeed',
    'PhysicalDamage',
    'FireDamage',
    'ColdDamage',
    'LightningDamage',
    'ChaosDamage',
    'AccuracyRating',
    'StunThreshold',
    'AllSpellSkills',
    'AllMinionSkills',
    'AllProjectileSkills',
  ];

  type Props = {
    goal: Goal;
    onUpdate: (next: Goal) => void;
    /** Optional list of concept ids surfaced as autocomplete options.
     * v1 ships a small static list; A.6's reload_bundle can pipe the
     * bundle's full concept list here once the SettingsPanel exposes it. */
    concepts?: string[];
  };

  let { goal, onUpdate, concepts = DEFAULT_CONCEPTS }: Props = $props();

  // ---- helpers -----------------------------------------------------
  function patchGoal(patch: Partial<Goal>) {
    onUpdate({ ...goal, ...patch });
  }

  function patchPrefix(idx: number, patch: Partial<TargetSpec>) {
    const prefixes = [...(goal.target.prefixes ?? [])];
    prefixes[idx] = { ...prefixes[idx], ...patch };
    patchGoal({ target: { ...goal.target, prefixes } });
  }

  function patchSuffix(idx: number, patch: Partial<TargetSpec>) {
    const suffixes = [...(goal.target.suffixes ?? [])];
    suffixes[idx] = { ...suffixes[idx], ...patch };
    patchGoal({ target: { ...goal.target, suffixes } });
  }

  function addPrefix() {
    const prefixes = [...(goal.target.prefixes ?? [])];
    prefixes.push({
      concept: 'EnergyShield',
      count: 1,
      min_tier: 1,
      allow_hybrid: true,
    });
    patchGoal({ target: { ...goal.target, prefixes } });
  }

  function addSuffix() {
    const suffixes = [...(goal.target.suffixes ?? [])];
    suffixes.push({
      concept_any: ['FireResistance', 'ColdResistance', 'LightningResistance'],
      count: 2,
      min_tier: 1,
      allow_hybrid: true,
    });
    patchGoal({ target: { ...goal.target, suffixes } });
  }

  function removePrefix(idx: number) {
    const prefixes = [...(goal.target.prefixes ?? [])];
    prefixes.splice(idx, 1);
    patchGoal({ target: { ...goal.target, prefixes } });
  }

  function removeSuffix(idx: number) {
    const suffixes = [...(goal.target.suffixes ?? [])];
    suffixes.splice(idx, 1);
    patchGoal({ target: { ...goal.target, suffixes } });
  }

  function patchBudget(patch: Partial<DivEquiv>) {
    patchGoal({ budget: { ...goal.budget, ...patch } });
  }

  function specConceptString(s: TargetSpec): string {
    if (s.concept) return s.concept;
    if (s.concept_any && s.concept_any.length > 0) return s.concept_any.join(',');
    return '';
  }

  function setSpecConcept(idx: number, raw: string, slot: 'prefix' | 'suffix') {
    const tokens = raw
      .split(',')
      .map((t) => t.trim())
      .filter(Boolean);
    const patch =
      tokens.length === 1
        ? { concept: tokens[0], concept_any: [] }
        : { concept: null, concept_any: tokens };
    if (slot === 'prefix') {
      patchPrefix(idx, patch);
    } else {
      patchSuffix(idx, patch);
    }
  }
</script>

<section class="target-panel">
  <h2>Target</h2>

  <div class="block">
    <div class="block-header">
      <h3>Prefixes</h3>
      <button class="add" onclick={addPrefix}>+ Add prefix spec</button>
    </div>
    {#each goal.target.prefixes ?? [] as spec, i (i)}
      <div class="spec">
        <input
          class="concept-input"
          type="text"
          value={specConceptString(spec)}
          onchange={(e) =>
            setSpecConcept(i, (e.currentTarget as HTMLInputElement).value, 'prefix')}
          placeholder="EnergyShield, Life, ..."
          list="concept-list"
        />
        <label class="num">
          count
          <input
            type="number"
            min="1"
            max="3"
            value={spec.count ?? 1}
            onchange={(e) =>
              patchPrefix(i, { count: Number((e.currentTarget as HTMLInputElement).value) })}
          />
        </label>
        <label class="num">
          min tier
          <input
            type="number"
            min="1"
            max="10"
            value={spec.min_tier ?? 1}
            onchange={(e) =>
              patchPrefix(i, {
                min_tier: Number((e.currentTarget as HTMLInputElement).value),
              })}
          />
        </label>
        <label class="checkbox">
          <input
            type="checkbox"
            checked={spec.allow_hybrid ?? true}
            onchange={(e) =>
              patchPrefix(i, {
                allow_hybrid: (e.currentTarget as HTMLInputElement).checked,
              })}
          />
          allow hybrid
        </label>
        <button class="remove" onclick={() => removePrefix(i)} title="Remove">
          ×
        </button>
      </div>
    {/each}
  </div>

  <div class="block">
    <div class="block-header">
      <h3>Suffixes</h3>
      <button class="add" onclick={addSuffix}>+ Add suffix spec</button>
    </div>
    {#each goal.target.suffixes ?? [] as spec, i (i)}
      <div class="spec">
        <input
          class="concept-input"
          type="text"
          value={specConceptString(spec)}
          onchange={(e) =>
            setSpecConcept(i, (e.currentTarget as HTMLInputElement).value, 'suffix')}
          placeholder="FireResistance, ColdResistance, ..."
          list="concept-list"
        />
        <label class="num">
          count
          <input
            type="number"
            min="1"
            max="3"
            value={spec.count ?? 1}
            onchange={(e) =>
              patchSuffix(i, { count: Number((e.currentTarget as HTMLInputElement).value) })}
          />
        </label>
        <label class="num">
          min tier
          <input
            type="number"
            min="1"
            max="10"
            value={spec.min_tier ?? 1}
            onchange={(e) =>
              patchSuffix(i, {
                min_tier: Number((e.currentTarget as HTMLInputElement).value),
              })}
          />
        </label>
        <label class="checkbox">
          <input
            type="checkbox"
            checked={spec.allow_hybrid ?? true}
            onchange={(e) =>
              patchSuffix(i, {
                allow_hybrid: (e.currentTarget as HTMLInputElement).checked,
              })}
          />
          allow hybrid
        </label>
        <button class="remove" onclick={() => removeSuffix(i)} title="Remove">
          ×
        </button>
      </div>
    {/each}
  </div>

  <div class="block">
    <h3>Budget (divine-equivalent)</h3>
    <div class="budget">
      <label class="num">
        min
        <input
          type="number"
          min="0"
          step="0.1"
          value={goal.budget.min}
          onchange={(e) =>
            patchBudget({ min: Number((e.currentTarget as HTMLInputElement).value) })}
        />
      </label>
      <label class="num">
        expected
        <input
          type="number"
          min="0"
          step="0.1"
          value={goal.budget.expected}
          onchange={(e) =>
            patchBudget({ expected: Number((e.currentTarget as HTMLInputElement).value) })}
        />
      </label>
      <label class="num">
        max
        <input
          type="number"
          min="0"
          step="0.1"
          value={goal.budget.max}
          onchange={(e) =>
            patchBudget({ max: Number((e.currentTarget as HTMLInputElement).value) })}
        />
      </label>
    </div>
  </div>

  <datalist id="concept-list">
    {#each concepts as c}
      <option value={c}></option>
    {/each}
  </datalist>
</section>

<style>
  .target-panel {
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
    margin: 0;
    font-size: 0.85rem;
    color: var(--fg-muted);
  }

  .block {
    margin-bottom: 0.75rem;
  }

  .block-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.4rem;
  }

  .add {
    background: transparent;
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 3px;
    font-size: 0.75rem;
    padding: 0.2rem 0.5rem;
    cursor: pointer;
  }

  .add:hover {
    background: var(--accent);
    color: var(--bg);
  }

  .spec {
    display: grid;
    grid-template-columns: 1fr auto auto auto auto;
    gap: 0.4rem;
    align-items: center;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 0.4rem;
    margin-bottom: 0.3rem;
    font-size: 0.8rem;
  }

  .concept-input {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--fg);
    padding: 0.25rem 0.4rem;
    border-radius: 2px;
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.8rem;
  }

  .num {
    display: flex;
    flex-direction: column;
    font-size: 0.7rem;
    color: var(--fg-muted);
  }

  .num input {
    width: 4rem;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--fg);
    padding: 0.15rem 0.3rem;
    border-radius: 2px;
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.8rem;
  }

  .checkbox {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    font-size: 0.7rem;
    color: var(--fg-muted);
  }

  .remove {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--fg-muted);
    border-radius: 2px;
    cursor: pointer;
    width: 1.5rem;
    height: 1.5rem;
    padding: 0;
  }

  .remove:hover {
    border-color: #ff8c8c;
    color: #ff8c8c;
  }

  .budget {
    display: flex;
    gap: 0.5rem;
  }
</style>


