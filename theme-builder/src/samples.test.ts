import { describe, expect, it } from 'vitest';
import { groupSamples } from './samples.ts';
import type { SampleInfo } from './types.ts';

const sample = (name: string, part?: boolean): SampleInfo => ({
  name,
  template: `${name}.html`,
  source: 'builtin',
  preview_url: `/preview/${name}`,
  part,
});

describe('groupSamples', () => {
  it('puts the parts in their own group after the pages', () => {
    const groups = groupSamples([
      sample('toolbar', true),
      sample('index'),
      sample('article', false),
    ]);
    expect(groups.map((g) => [g.part, g.items.map((s) => s.name)])).toEqual([
      [false, ['index', 'article']],
      [true, ['toolbar']],
    ]);
  });

  it('keeps one group when there are no parts, and treats a missing flag as a page', () => {
    const groups = groupSamples([sample('index'), sample('page')]);
    expect(groups).toHaveLength(1);
    expect(groups[0].part).toBe(false);
  });

  it('returns nothing for no samples', () => {
    expect(groupSamples([])).toEqual([]);
  });
});
