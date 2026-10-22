import type { ContentItem } from '../types';

/** Returns the category's `color` field when it is a CSS hex color */
export const categoryColor = (category: ContentItem): string | undefined => {
  const color = category.fields?.color;
  return typeof color === 'string' && /^#[0-9a-fA-F]{3,8}$/.test(color) ? color : undefined;
};

/** Returns the `category_id` relation of a content item, or `undefined` when it has none */
export const categoryIdOf = (item: ContentItem): string | undefined => {
  const id = item.fields?.category_id;
  return typeof id === 'string' && id !== '' ? id : undefined;
};
