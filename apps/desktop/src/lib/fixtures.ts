// Seed fixtures the user can load with one click.

import type { Goal, Item } from './types';

/// Fresh Normal ilvl 82 BodyArmour — starting state of the user's
/// canonical worked example.
export const FRESH_BODY_ARMOUR: Item = {
  base: 'BodyArmour',
  ilvl: 82,
  rarity: 'normal',
  corrupted: false,
  sanctified: false,
  mirrored: false,
  quality: 0,
  quality_kind: 'Untagged',
  implicits: [],
  prefixes: [],
  suffixes: [],
  enchantments: [],
  hidden_desecrated: null,
  sockets: [],
  hinekora_lock: null,
};

/// Triple T1 ES + dual T1 res suffix goal — the user's worked example.
export const WORKED_EXAMPLE_GOAL: Goal = {
  target: {
    prefixes: [
      {
        concept: 'EnergyShield',
        count: 3,
        min_tier: 1,
        allow_hybrid: true,
      },
    ],
    suffixes: [
      {
        concept_any: [
          'FireResistance',
          'ColdResistance',
          'LightningResistance',
          'AllResistances',
        ],
        count: 2,
        min_tier: 1,
        allow_hybrid: true,
      },
    ],
    constraints: [],
  },
  abandon_criteria: [{ corrupted: true }, { sanctified: true }],
  budget: { min: 40, expected: 100, max: 200 },
};
