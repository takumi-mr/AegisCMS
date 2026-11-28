/** One thing a store package will be allowed to do, shown before it is installed */
export type StoreCapability =
  | { kind: 'hooks'; values: string[] }
  | { kind: 'storage' }
  | { kind: 'network'; values: string[] }
  | { kind: 'frames'; values: string[] }
  | { kind: 'siteFrames' }
  | { kind: 'scripts'; inline: boolean; local: boolean }
  | { kind: 'csp'; directive: string; values: string[] };

const strings = (value: unknown): string[] =>
  Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : [];

const object = (value: unknown): Record<string, unknown> =>
  value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};

/**
 * The capabilities a catalog lists for a version (a plugin manifest's `capabilities`, or `{ csp }`
 * for a theme), as items to show before installing. Unknown keys are left out; empty ones too.
 */
export function storeCapabilities(capabilities: Record<string, unknown>): StoreCapability[] {
  const items: StoreCapability[] = [];
  const hooks = strings(capabilities.hooks);
  if (hooks.length > 0) items.push({ kind: 'hooks', values: hooks });
  if (capabilities.storage === true) items.push({ kind: 'storage' });
  const network = strings(object(capabilities.network).allowed_domains);
  if (network.length > 0) items.push({ kind: 'network', values: network });
  const embed = object(capabilities.embed);
  const frames = strings(embed.frame_domains);
  if (frames.length > 0) items.push({ kind: 'frames', values: frames });
  if (embed.site_domains === true) items.push({ kind: 'siteFrames' });
  const inline = capabilities.inline_scripts === true;
  const local = capabilities.local_scripts === true;
  if (inline || local) items.push({ kind: 'scripts', inline, local });
  // `script_domains` → `script-src`, `font_domains` → `font-src`, ...
  for (const [key, value] of Object.entries(object(capabilities.csp))) {
    const values = strings(value);
    if (key.endsWith('_domains') && values.length > 0) {
      items.push({ kind: 'csp', directive: `${key.slice(0, -'_domains'.length)}-src`, values });
    }
  }
  return items;
}
