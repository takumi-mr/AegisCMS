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

/** A `datetime-local` value (local time, no zone) as an RFC 3339 UTC timestamp; undefined when empty or invalid */
export const fromDateTimeLocal = (local: string): string | undefined => {
  if (!local) return undefined;
  const date = new Date(local);
  return Number.isNaN(date.getTime()) ? undefined : date.toISOString();
};

/** Date only, in the Admin Studio language */
export const formatDate = (iso: string): string => {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleDateString(currentLanguage());
};
