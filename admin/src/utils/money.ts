import { currentLanguage } from '../i18n';

/**
 * Formats an amount in the currency's minor unit (yen for JPY, cents for USD), the unit the
 * shop API uses everywhere
 */
export const formatMoney = (amount: number, currency = 'jpy'): string => {
  try {
    const format = new Intl.NumberFormat(currentLanguage(), {
      style: 'currency',
      currency: currency.toUpperCase(),
    });
    const digits = format.resolvedOptions().maximumFractionDigits ?? 0;
    return format.format(amount / 10 ** digits);
  } catch {
    return `${currency.toUpperCase()} ${amount}`;
  }
};
