<script lang="ts">
  import { invoke } from './tauri';
  import { displayId } from './assets';
  import type {
    DivEquiv,
    EligibleModView,
    EligibleModsResponse,
    Goal,
    Item,
    TargetSpec,
  } from './types';

  type Props = {
    goal: Goal;
    item: Item;
    onUpdate: (next: Goal) => void;
    onReset: () => void;
  };

  let { goal, item, onUpdate, onReset }: Props = $props();

  type ConceptOption = {
    id: string;
    label: string;
    affixHint: 'prefix' | 'suffix';
  };

  const concepts: ConceptOption[] = [
    { id: 'EnergyShield', label: 'Energy Shield', affixHint: 'prefix' },
    { id: 'Life', label: 'Life', affixHint: 'prefix' },
    { id: 'Mana', label: 'Mana', affixHint: 'prefix' },
    { id: 'Armour', label: 'Armour', affixHint: 'prefix' },
    { id: 'Evasion', label: 'Evasion', affixHint: 'prefix' },
    { id: 'SpellDamage', label: 'Spell Damage', affixHint: 'prefix' },
    { id: 'AttackDamage', label: 'Attack Damage', affixHint: 'prefix' },
    { id: 'PhysicalDamage', label: 'Physical Damage', affixHint: 'prefix' },
    { id: 'FireDamage', label: 'Fire Damage', affixHint: 'prefix' },
    { id: 'ColdDamage', label: 'Cold Damage', affixHint: 'prefix' },
    { id: 'LightningDamage', label: 'Lightning Damage', affixHint: 'prefix' },
    { id: 'FireResistance', label: 'Fire Resistance', affixHint: 'suffix' },
    { id: 'ColdResistance', label: 'Cold Resistance', affixHint: 'suffix' },
    { id: 'LightningResistance', label: 'Lightning Resistance', affixHint: 'suffix' },
    { id: 'ChaosResistance', label: 'Chaos Resistance', affixHint: 'suffix' },
    { id: 'AllResistances', label: 'All Resistances', affixHint: 'suffix' },
    { id: 'Strength', label: 'Strength', affixHint: 'suffix' },
    { id: 'Dexterity', label: 'Dexterity', affixHint: 'suffix' },
    { id: 'Intelligence', label: 'Intelligence', affixHint: 'suffix' },
    { id: 'AllAttributes', label: 'All Attributes', affixHint: 'suffix' },
    { id: 'AttackSpeed', label: 'Attack Speed', affixHint: 'suffix' },
    { id: 'CastSpeed', label: 'Cast Speed', affixHint: 'suffix' },
    { id: 'CriticalStrikeChance', label: 'Crit Chance', affixHint: 'suffix' },
    { id: 'CriticalStrikeMultiplier', label: 'Crit Multiplier', affixHint: 'suffix' },
    { id: 'MovementSpeed', label: 'Movement Speed', affixHint: 'suffix' },
    { id: 'AccuracyRating', label: 'Accuracy', affixHint: 'suffix' },
  ];

  type Preset = {
    id: string;
    label: string;
    description: string;
    apply: () => Goal;
  };

  const presets: Preset[] = [
    {
      id: '3xt1-es-2res',
      label: 'Triple T1 ES + Dual Resists',
      description: 'High-ES caster body armour with two resist suffixes.',
      apply: () => ({
        target: {
          prefixes: [
            { concept: 'EnergyShield', count: 3, min_tier: 1, allow_hybrid: true },
          ],
          suffixes: [
            {
              concept_any: ['FireResistance', 'ColdResistance', 'LightningResistance', 'AllResistances'],
              count: 2,
              min_tier: 1,
              allow_hybrid: true,
            },
          ],
          constraints: [],
        },
        abandon_criteria: [{ corrupted: true }, { sanctified: true }],
        budget: { min: 40, expected: 100, max: 200 },
      }),
    },
    {
      id: 'tri-res-life',
      label: 'Tri-Res + Life',
      description: 'Balanced gear: life prefix and three resists.',
      apply: () => ({
        target: {
          prefixes: [{ concept: 'Life', count: 1, min_tier: 1, allow_hybrid: true }],
          suffixes: [
            {
              concept_any: ['FireResistance', 'ColdResistance', 'LightningResistance'],
              count: 3,
              min_tier: 1,
              allow_hybrid: true,
            },
          ],
          constraints: [],
        },
        abandon_criteria: [{ corrupted: true }],
        budget: { min: 10, expected: 30, max: 80 },
      }),
    },
    {
      id: 'caster-es',
      label: 'ilvl 82 Caster ES',
      description: 'Top-tier ES + spell damage on a high ilvl caster base.',
      apply: () => ({
        target: {
          prefixes: [
            { concept: 'EnergyShield', count: 2, min_tier: 1, allow_hybrid: true },
            { concept: 'SpellDamage', count: 1, min_tier: 2, allow_hybrid: false },
          ],
          suffixes: [
            { concept: 'CastSpeed', count: 1, min_tier: 2, allow_hybrid: false },
            {
              concept_any: ['FireResistance', 'ColdResistance', 'LightningResistance'],
              count: 2,
              min_tier: 2,
              allow_hybrid: true,
            },
          ],
          constraints: [],
        },
        abandon_criteria: [{ corrupted: true }],
        budget: { min: 25, expected: 75, max: 200 },
      }),
    },
  ];

  let eligibleByAffix = $state<EligibleModsResponse | null>(null);
  let eligibleLoading = $state(false);
  let eligibleError = $state<string | null>(null);

  $effect(() => {
    void item;
    eligibleLoading = true;
    eligibleError = null;
    invoke<EligibleModsResponse>('eligible_mods', {
      args: { item, affix: 'either', min_required_level: 0 },
    })
      .then((r) => {
        eligibleByAffix = r;
      })
      .catch((e) => {
        eligibleError = String(e);
      })
      .finally(() => {
        eligibleLoading = false;
      });
  });

  const targetConcepts = $derived(getTargetConcepts(goal));
  const supportedMods = $derived(supportedEligibleMods(eligibleByAffix?.mods ?? [], targetConcepts));

  function getTargetConcepts(g: Goal): string[] {
    const out: string[] = [];
    const collect = (specs: TargetSpec[] | undefined) => {
      for (const s of specs ?? []) {
        if (s.concept) out.push(s.concept);
        if (s.concept_any) out.push(...s.concept_any);
      }
    };
    collect(g.target.prefixes);
    collect(g.target.suffixes);
    return [...new Set(out)];
  }

  function supportedEligibleMods(mods: EligibleModView[], wanted: string[]): EligibleModView[] {
    if (wanted.length === 0) return [];
    return mods
      .filter((m) => m.eligible_now)
      .filter((m) => m.concepts.some((c) => wanted.includes(c)))
      .slice(0, 12);
  }

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

  function addConcept(concept: ConceptOption) {
    if (concept.affixHint === 'prefix') {
      const prefixes = [...(goal.target.prefixes ?? [])];
      prefixes.push({
        concept: concept.id,
        count: 1,
        min_tier: 1,
        allow_hybrid: true,
      });
      patchGoal({ target: { ...goal.target, prefixes } });
    } else {
      const suffixes = [...(goal.target.suffixes ?? [])];
      suffixes.push({
        concept: concept.id,
        count: 1,
        min_tier: 1,
        allow_hybrid: true,
      });
      patchGoal({ target: { ...goal.target, suffixes } });
    }
  }

  function patchBudget(patch: Partial<DivEquiv>) {
    patchGoal({ budget: { ...goal.budget, ...patch } });
  }

  function applyPreset(preset: Preset) {
    onUpdate(preset.apply());
  }

  function specLabel(s: TargetSpec): string {
    if (s.concept) return displayId(s.concept);
    if (s.concept_any && s.concept_any.length > 0) {
      return s.concept_any.map((c) => displayId(c)).join(' / ');
    }
    return '(unset)';
  }
</script>

<div class="builder">
  <section class="block">
    <h4>Quick presets</h4>
    <div class="presets">
      {#each presets as p (p.id)}
        <button class="preset" type="button" onclick={() => applyPreset(p)}>
          <strong>{p.label}</strong>
          <span>{p.description}</span>
        </button>
      {/each}
    </div>
  </section>

  <section class="block">
    <h4>What do you want?</h4>
    <p class="muted small">
      Pick concepts. We'll add them as prefix or suffix specs based on
      whether they roll on each side of the affix pool.
    </p>
    <div class="concept-grid">
      {#each concepts as c (c.id)}
        <button
          type="button"
          class="concept-btn"
          class:active={targetConcepts.includes(c.id)}
          onclick={() => addConcept(c)}
        >
          <span>{c.label}</span>
          <em>{c.affixHint}</em>
        </button>
      {/each}
    </div>
  </section>

  <section class="block">
    <h4>Prefix specs ({(goal.target.prefixes ?? []).length})</h4>
    {#each goal.target.prefixes ?? [] as spec, i (i)}
      <div class="spec">
        <strong>{specLabel(spec)}</strong>
        <label>
          count
          <input
            type="number"
            min="1"
            max="3"
            value={spec.count ?? 1}
            onchange={(e) =>
              patchPrefix(i, {
                count: Number((e.currentTarget as HTMLInputElement).value),
              })}
          />
        </label>
        <label>
          tier ≥
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
        <label class="check">
          <input
            type="checkbox"
            checked={spec.allow_hybrid ?? true}
            onchange={(e) =>
              patchPrefix(i, {
                allow_hybrid: (e.currentTarget as HTMLInputElement).checked,
              })}
          />
          hybrid
        </label>
        <button class="remove" onclick={() => removePrefix(i)} title="Remove">×</button>
      </div>
    {/each}
    {#if (goal.target.prefixes ?? []).length === 0}
      <p class="muted small">No prefix specs yet.</p>
    {/if}
  </section>

  <section class="block">
    <h4>Suffix specs ({(goal.target.suffixes ?? []).length})</h4>
    {#each goal.target.suffixes ?? [] as spec, i (i)}
      <div class="spec">
        <strong>{specLabel(spec)}</strong>
        <label>
          count
          <input
            type="number"
            min="1"
            max="3"
            value={spec.count ?? 1}
            onchange={(e) =>
              patchSuffix(i, {
                count: Number((e.currentTarget as HTMLInputElement).value),
              })}
          />
        </label>
        <label>
          tier ≥
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
        <label class="check">
          <input
            type="checkbox"
            checked={spec.allow_hybrid ?? true}
            onchange={(e) =>
              patchSuffix(i, {
                allow_hybrid: (e.currentTarget as HTMLInputElement).checked,
              })}
          />
          hybrid
        </label>
        <button class="remove" onclick={() => removeSuffix(i)} title="Remove">×</button>
      </div>
    {/each}
    {#if (goal.target.suffixes ?? []).length === 0}
      <p class="muted small">No suffix specs yet.</p>
    {/if}
  </section>

  <section class="block">
    <h4>Budget (divine-equivalent)</h4>
    <div class="budget">
      <label>
        min
        <input
          type="number"
          min="0"
          step="0.5"
          value={goal.budget.min}
          onchange={(e) =>
            patchBudget({ min: Number((e.currentTarget as HTMLInputElement).value) })}
        />
      </label>
      <label>
        expected
        <input
          type="number"
          min="0"
          step="0.5"
          value={goal.budget.expected}
          onchange={(e) =>
            patchBudget({
              expected: Number((e.currentTarget as HTMLInputElement).value),
            })}
        />
      </label>
      <label>
        max
        <input
          type="number"
          min="0"
          step="0.5"
          value={goal.budget.max}
          onchange={(e) =>
            patchBudget({ max: Number((e.currentTarget as HTMLInputElement).value) })}
        />
      </label>
    </div>
  </section>

  <section class="block">
    <h4>Eligible mods supporting your target</h4>
    {#if eligibleLoading}
      <p class="muted small">Loading…</p>
    {:else if eligibleError}
      <p class="error small">{eligibleError}</p>
    {:else if !eligibleByAffix?.data_available}
      <p class="muted small">
        No mod data bundled for {eligibleByAffix?.item_class ?? item.base}. Eligible-mod preview
        unavailable for this class.
      </p>
    {:else if targetConcepts.length === 0}
      <p class="muted small">Pick at least one concept to see supporting mods.</p>
    {:else if supportedMods.length === 0}
      <p class="muted small">
        None of the eligible mods on this base satisfy the chosen concepts at the current ilvl.
      </p>
    {:else}
      <ul class="support-list">
        {#each supportedMods as m (m.mod_id)}
          <li>
            <header>
              <span class="tier">T{m.tier_index}/{m.tier_count}</span>
              <span class="mod-name">{m.name ?? m.mod_id}</span>
              <span class="affix">{m.affix_type}</span>
              <span class="ilvl">ilvl {m.required_level}</span>
            </header>
            {#if m.text_template}<p class="tpl">{m.text_template}</p>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <button class="ghost wide" type="button" onclick={onReset}>Reset target</button>
</div>

<style>
  .builder {
    display: grid;
    gap: 0.7rem;
  }

  h4 {
    margin: 0 0 0.4rem;
    color: var(--gold);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.78rem;
  }

  .block {
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.3);
    padding: 0.6rem;
  }

  .small {
    font-size: 0.78rem;
  }

  .muted {
    color: var(--fg-muted);
    margin: 0;
  }

  .presets {
    display: grid;
    grid-template-columns: 1fr;
    gap: 0.4rem;
  }

  .preset {
    text-align: left;
    background: rgba(0, 0, 0, 0.4);
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.5rem 0.65rem;
    color: var(--fg);
    cursor: pointer;
  }

  .preset strong {
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
    display: block;
    margin-bottom: 0.15rem;
  }

  .preset span {
    color: var(--fg-muted);
    font-size: 0.78rem;
  }

  .preset:hover {
    border-color: var(--border-gold);
  }

  .concept-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 0.35rem;
  }

  .concept-btn {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: rgba(0, 0, 0, 0.4);
    color: var(--fg);
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.4rem 0.55rem;
    cursor: pointer;
    font-size: 0.85rem;
  }

  .concept-btn em {
    font-style: normal;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--fg-muted);
  }

  .concept-btn.active {
    border-color: var(--gold);
    color: var(--gold-bright);
    background: linear-gradient(90deg, rgba(197, 143, 61, 0.25), rgba(20, 14, 5, 0.7));
  }

  .spec {
    display: grid;
    grid-template-columns: 1.4fr auto auto auto auto;
    gap: 0.5rem;
    align-items: center;
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.4rem 0.55rem;
    margin-bottom: 0.3rem;
  }

  .spec strong {
    color: var(--fg);
  }

  .spec label {
    display: flex;
    flex-direction: column;
    font-size: 0.72rem;
    color: var(--fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .spec label.check {
    flex-direction: row;
    align-items: center;
    gap: 0.3rem;
    text-transform: none;
    color: var(--fg-muted);
  }

  .spec input[type='number'] {
    background: rgba(0, 0, 0, 0.5);
    color: var(--fg);
    border: 1px solid var(--border-strong);
    border-radius: 3px;
    padding: 0.2rem 0.35rem;
    width: 4rem;
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.82rem;
  }

  .remove {
    background: transparent;
    border: 1px solid var(--border-strong);
    color: var(--fg-muted);
    border-radius: 3px;
    width: 1.6rem;
    height: 1.6rem;
    cursor: pointer;
  }

  .remove:hover {
    border-color: #ff8c8c;
    color: #ff8c8c;
  }

  .budget {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .budget label {
    display: flex;
    flex-direction: column;
    font-size: 0.72rem;
    color: var(--fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .budget input {
    width: 5.5rem;
    background: rgba(0, 0, 0, 0.5);
    color: var(--fg);
    border: 1px solid var(--border-strong);
    border-radius: 3px;
    padding: 0.25rem 0.4rem;
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.85rem;
  }

  .support-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.35rem;
  }

  .support-list li {
    border: 1px solid rgba(197, 143, 61, 0.3);
    background: rgba(0, 0, 0, 0.3);
    border-radius: 4px;
    padding: 0.4rem 0.55rem;
  }

  .support-list header {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .support-list .tier {
    font-family: Georgia, 'Times New Roman', serif;
    color: var(--gold-bright);
    font-size: 0.85rem;
  }

  .support-list .mod-name {
    color: var(--fg);
    font-weight: 600;
  }

  .support-list .affix,
  .support-list .ilvl {
    color: var(--fg-muted);
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .support-list .tpl {
    margin: 0.25rem 0 0;
    color: #00c8ff;
    font-size: 0.78rem;
    white-space: pre-wrap;
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

  .wide {
    width: 100%;
  }

  .error {
    color: #ff8c8c;
    margin: 0;
  }
</style>
