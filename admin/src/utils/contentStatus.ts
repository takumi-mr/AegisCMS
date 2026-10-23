/** Content states with a translated label (`common.status.*`); others show as is */
const LABELED_STATUSES = ['draft', 'scheduled', 'published'] as const;

export const isLabeledStatus = (status: string): status is (typeof LABELED_STATUSES)[number] =>
  (LABELED_STATUSES as readonly string[]).includes(status);
