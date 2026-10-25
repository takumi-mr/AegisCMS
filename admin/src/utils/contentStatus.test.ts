import { describe, expect, it } from 'vitest';

import { isLabeledStatus } from './contentStatus';

describe('contentStatus utility', () => {
  it('identifies draft, scheduled, and published as labeled statuses', () => {
    expect(isLabeledStatus('draft')).toBe(true);
    expect(isLabeledStatus('scheduled')).toBe(true);
    expect(isLabeledStatus('published')).toBe(true);
  });

  it('returns false for unlabeled or custom statuses', () => {
    expect(isLabeledStatus('archived')).toBe(false);
    expect(isLabeledStatus('deleted')).toBe(false);
    expect(isLabeledStatus('pending')).toBe(false);
    expect(isLabeledStatus('')).toBe(false);
  });
});
