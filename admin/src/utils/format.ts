import { currentLanguage } from '../i18n';

/** Date and time in the Admin Studio language (`iso` as is when it is not a date) */
export const formatDateTime = (iso: string): string => {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString(currentLanguage());
};

/** Date only, in the Admin Studio language */
export const formatDate = (iso: string): string => {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleDateString(currentLanguage());
};
