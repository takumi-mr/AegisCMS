import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

// The color theme lives in the tokens of index.css. These checks keep the stylesheets from drifting
// away from it: a color written into a component stays the same in the light theme

const SRC = new URL('..', import.meta.url).pathname;

const cssFiles = (dir: string): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return cssFiles(path);
    return entry.name.endsWith('.css') ? [path] : [];
  });

const indexCss = readFileSync(join(SRC, 'index.css'), 'utf8');
const componentCss = cssFiles(SRC).filter((path) => !path.endsWith('/index.css'));

const lightStart = indexCss.indexOf(":root[data-theme='light']");
const darkBlock = indexCss.slice(indexCss.indexOf(':root {'), lightStart);
const lightBlock = indexCss.slice(lightStart);

const definedIn = (css: string) =>
  new Set([...css.matchAll(/^\s*(--[a-zA-Z0-9-]+):/gm)].map((match) => match[1]));

describe('design tokens', () => {
  it('has a light block that follows the dark one', () => {
    expect(lightStart).toBeGreaterThan(0);
    expect(lightBlock).toContain('color-scheme: light');
    expect(darkBlock).toContain('color-scheme: dark');
  });

  it('gives the light theme its own value for every color token', () => {
    const light = definedIn(lightBlock);
    // Not colors (the same in both themes), or aliases of other tokens
    const shared = /^--(font-|radius-|transition-)/;
    const aliases = new Set(['--accent', '--bg-surface-hover']);
    const missing = [...definedIn(darkBlock)].filter(
      (name) => !shared.test(name) && !aliases.has(name) && !light.has(name)
    );
    expect(missing).toEqual([]);
  });

  it('defines every token the components use (an undefined one silently falls back or vanishes)', () => {
    const defined = definedIn(indexCss);
    const problems: string[] = [];
    for (const path of componentCss) {
      const css = readFileSync(path, 'utf8');
      const local = definedIn(css);
      for (const match of css.matchAll(/var\((--[a-zA-Z0-9-]+)/g)) {
        if (!defined.has(match[1]) && !local.has(match[1])) {
          problems.push(`${path.replace(SRC, '')}: ${match[1]}`);
        }
      }
    }
    expect([...new Set(problems)]).toEqual([]);
  });

  it('writes no translucent white into components (invisible on the light theme): use --overlay-rgb', () => {
    const offenders = componentCss.filter((path) =>
      /rgba\(\s*255\s*,\s*255\s*,\s*255\s*,/.test(readFileSync(path, 'utf8'))
    );
    expect(offenders.map((path) => path.replace(SRC, ''))).toEqual([]);
  });
});
