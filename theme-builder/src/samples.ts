import type { SampleInfo } from './types.ts';

export interface SampleGroup {
  /** 部品（ほかのテンプレートから読み込まれる断片）のグループか */
  part: boolean;
  items: SampleInfo[];
}

/** ページと部品に分ける。部品がなければグループは 1 つだけ。空のグループは返さない */
export function groupSamples(samples: SampleInfo[]): SampleGroup[] {
  const groups: SampleGroup[] = [
    { part: false, items: samples.filter((s) => !s.part) },
    { part: true, items: samples.filter((s) => s.part) },
  ];
  return groups.filter((g) => g.items.length > 0);
}
