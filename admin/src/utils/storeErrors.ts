import i18n from '../i18n';

/** The `code`s of the store API (`handlers/store.rs`), translated under `store.errors` */
const ERROR_CODES = [
  'store_disabled',
  'catalog_not_found',
  'catalog_fetch_failed',
  'catalog_bad_signature',
  'catalog_key_rotated',
  'catalog_malformed',
  'catalog_unsupported_schema',
  'catalog_expired',
  'catalog_rollback',
  'package_not_found',
  'version_not_found',
  'incompatible_version',
  'package_fetch_failed',
  'package_mismatch',
  'package_invalid',
  'plugins_disabled',
] as const;

const isKnownCode = (code: string | undefined): code is (typeof ERROR_CODES)[number] =>
  (ERROR_CODES as readonly string[]).includes(code ?? '');

/** The translated message for a store API `code`, or the server's English text */
export const errorText = (
  code: string | undefined,
  message: string | undefined,
  fallback: string
): string => (isKnownCode(code) ? i18n.t(`store.errors.${code}`) : message || fallback);

/** The translated reason for a store API `code` (also used by the update check) */
export const storeErrorText = (code: string | undefined, message: string | undefined): string =>
  errorText(code, message, i18n.t('api.storeFailed'));
