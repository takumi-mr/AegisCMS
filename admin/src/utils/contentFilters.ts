/** `category` value for items without a category (the API's `category=none`) */
export const UNCATEGORIZED = 'none';

/** How the content list is searched and filtered on the server. An empty string means "no condition" */
export interface ContentListFilters {
  /** Search words: a partial match on the title, the slug and the excerpt */
  q: string;
  /** A category id, `none` (uncategorized) or empty (all) */
  category: string;
  /** A tag slug, or empty (all) */
  tag: string;
  /** `draft` / `scheduled` / `published`, or empty (all) */
  status: string;
}

export const NO_FILTERS: ContentListFilters = { q: '', category: '', tag: '', status: '' };

/** The statuses to filter by (the others, such as `archived`, are not offered) */
export const STATUS_FILTERS = ['draft', 'scheduled', 'published'] as const;

/** Whether any search word or filter narrows the list (`category` only counts for articles) */
export const isFiltered = (filters: ContentListFilters, isArticles: boolean): boolean =>
  filters.q.trim() !== '' ||
  (isArticles && filters.category !== '') ||
  filters.tag !== '' ||
  filters.status !== '';
