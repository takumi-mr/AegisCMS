import { currentLanguage } from '../i18n';

/** Date and time in the Admin Studio language (`iso` as is when it is not a date) */
export const formatDateTime = (iso: string): string => {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString(currentLanguage());
};

/** `iso` as the local `YYYY-MM-DDTHH:mm` an `<input type="datetime-local">` takes ('' when invalid) */
export const toDateTimeLocal = (iso: string): string => {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(
    date.getHours()
  )}:${pad(date.getMinutes())}`;
};

/** Date only, in the Admin Studio language */
export const formatDate = (iso: string): string => {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleDateString(currentLanguage());
};
