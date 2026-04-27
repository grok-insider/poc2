import type { AdvisorAction, AssetEntry, Item } from './types';

export type AssetIndex = Map<string, AssetEntry>;

export function buildAssetIndex(entries: AssetEntry[]): AssetIndex {
  const index = new Map<string, AssetEntry>();
  for (const entry of entries) {
    index.set(entry.id, entry);
    index.set(entry.name, entry);
  }
  return index;
}

export function assetUrl(index: AssetIndex, id: string | null | undefined): string | null {
  if (!id) return null;
  const entry = index.get(id);
  const sourceUrl = entry?.source_url;
  if (sourceUrl?.startsWith('http://') || sourceUrl?.startsWith('https://')) return sourceUrl;
  return null;
}

export function actionAssetId(action: AdvisorAction): string | null {
  switch (action.kind) {
    case 'apply_currency':
      return action.currency;
    case 'activate_omen':
      return action.omen;
    case 'apply_hinekoras_lock':
      return 'HinekorasLock';
    case 'reveal':
      return 'WellOfSouls';
    case 'recombine':
      return 'Recombinator';
    case 'stop':
      return 'Complete';
    case 'abandon':
      return 'Risk';
    case 'guidance':
      return 'Guidance';
    case 'recurring':
      return 'Loop';
  }
}

export function itemAssetId(item: Item): string | null {
  const base = item.base;
  const lower = base.toLowerCase();
  if (lower.includes('helmet') || lower.includes('circlet') || lower.includes('hood')) return 'Helmet';
  if (lower.includes('body') || lower.includes('armour') || lower.includes('robe') || lower.includes('vest')) return 'BodyArmour';
  if (lower.includes('glove') || lower.includes('gauntlet')) return 'Gloves';
  if (lower.includes('boot') || lower.includes('greave') || lower.includes('shoe')) return 'Boots';
  if (lower.includes('crossbow')) return 'Crossbow';
  if (lower.includes('bow')) return 'Bow';
  if (lower.includes('quarterstaff')) return 'Quarterstaff';
  if (lower.includes('staff')) return 'Staff';
  if (lower.includes('shield')) return 'Shield';
  if (lower.includes('focus')) return 'Focus';
  if (lower.includes('quiver')) return 'Quiver';
  if (lower.includes('ring')) return 'Ring';
  if (lower.includes('amulet')) return 'Amulet';
  if (lower.includes('belt') || lower.includes('sash') || lower.includes('girdle')) return 'Belt';
  if (lower.includes('sword')) return lower.includes('two hand') ? 'TwoHandSword' : 'OneHandSword';
  if (lower.includes('axe')) return lower.includes('two hand') ? 'TwoHandAxe' : 'OneHandAxe';
  if (lower.includes('mace')) return lower.includes('two hand') ? 'TwoHandMace' : 'OneHandMace';
  if (lower.includes('wand')) return 'Wand';
  if (lower.includes('sceptre') || lower.includes('scepter')) return 'Sceptre';
  if (lower.includes('spear')) return 'Spear';
  if (lower.includes('flail')) return 'Flail';
  if (lower.includes('claw')) return 'Claw';
  if (lower.includes('dagger')) return 'Dagger';
  return base || null;
}

export function initials(label: string): string {
  const letters = label
    .split(/\s+|_|-/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? '')
    .join('');
  return letters || '?';
}

export function displayId(id: string): string {
  return id
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/_/g, ' ')
    .trim();
}
