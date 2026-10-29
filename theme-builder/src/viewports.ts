export const VIEWPORTS = [
  { id: 'fit', label: '幅に合わせる', width: null },
  { id: '1280', label: '1280px', width: 1280 },
  { id: '768', label: '768px', width: 768 },
  { id: '390', label: '390px', width: 390 },
  { id: '320', label: '320px', width: 320 },
] as const;

export type ViewportId = (typeof VIEWPORTS)[number]['id'];
