import React, { useCallback, useEffect, useState } from 'react';
import { Check, Copy, KeyRound, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type { ApiToken, CreatedApiToken } from '../types';
import { useRoleName } from '../i18n/roles';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import userStyles from './UserForms.module.css';
import styles from './ApiTokenManager.module.css';

interface ApiTokenManagerProps {
  /** The user's own role: a token can never be given a role above it */
  maxRole: string;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

const TOKEN_ROLES: ApiToken['role'][] = ['viewer', 'contributor', 'reviewer', 'editor'];
const ROLE_LEVEL: Record<string, number> = {
  viewer: 1,
  contributor: 2,
  reviewer: 3,
  editor: 4,
  admin: 5,
};

/** Expiry choices: days, or `null` for no expiry. 90 days is the default */
const EXPIRY_CHOICES = [
  { key: 'days30', days: 30 },
  { key: 'days90', days: 90 },
  { key: 'days365', days: 365 },
  { key: 'never', days: null },
] as const;
type ExpiryKey = (typeof EXPIRY_CHOICES)[number]['key'];

const formatDate = (iso: string) => new Date(iso).toLocaleDateString();

/** The user's API tokens: issue (shown once), list and revoke */
export const ApiTokenManager: React.FC<ApiTokenManagerProps> = ({ maxRole, onShowToast }) => {
  const { t } = useTranslation();
  const roleName = useRoleName();
  const [tokens, setTokens] = useState<ApiToken[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [name, setName] = useState('');
  const [role, setRole] = useState<ApiToken['role']>('viewer');
  const [expiryKey, setExpiryKey] = useState<ExpiryKey>('days90');
  const [isCreating, setIsCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const [created, setCreated] = useState<CreatedApiToken | null>(null);
  const [copied, setCopied] = useState(false);

  const allowedRoles = TOKEN_ROLES.filter((r) => ROLE_LEVEL[r] <= (ROLE_LEVEL[maxRole] ?? 0));

  const load = useCallback(async () => {
    try {
      setTokens(await aegisApi.listApiTokens());
      setLoadError(null);
    } catch (err) {
      setLoadError(err instanceof Error ? err.message : t('profile.apiTokens.loadFailed'));
    }
  }, [t]);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listApiTokens()
      .then((list) => {
        if (!cancelled) setTokens(list);
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setLoadError(err instanceof Error ? err.message : t('profile.apiTokens.loadFailed'));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [t]);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsCreating(true);
    setCreateError(null);
    try {
      const days = EXPIRY_CHOICES.find((c) => c.key === expiryKey)?.days ?? null;
      const result = await aegisApi.createApiToken({
        name: name.trim(),
        role,
        expires_in_days: days,
      });
      setCreated(result);
      setName('');
      onShowToast(t('profile.apiTokens.created'));
      await load();
    } catch (err) {
      setCreateError(err instanceof Error ? err.message : t('profile.apiTokens.createFailed'));
    } finally {
      setIsCreating(false);
    }
  };

  const handleCopy = () => {
    if (!created) return;
    navigator.clipboard.writeText(created.token).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  const handleRevoke = async (token: ApiToken) => {
    if (!window.confirm(t('profile.apiTokens.revokeConfirm', { name: token.name }))) return;
    try {
      await aegisApi.revokeApiToken(token.id);
      onShowToast(t('profile.apiTokens.revoked'));
      await load();
    } catch (err) {
      onShowToast(
        err instanceof Error ? err.message : t('profile.apiTokens.revokeFailed'),
        'error'
      );
    }
  };

  return (
    <div className={userStyles.section}>
      <div className={userStyles.sectionTitle}>{t('profile.apiTokens.title')}</div>
      <span className={userStyles.hint}>{t('profile.apiTokens.hint')}</span>

      {created && (
        <div className={styles.created} role="status">
          <div className={styles.warning}>{t('profile.apiTokens.copyWarning')}</div>
          <code className={styles.tokenValue} data-testid="created-token">
            {created.token}
          </code>
          <div className={userStyles.actions}>
            <Button type="button" variant="secondary" onClick={handleCopy}>
              {copied ? <Check size={16} /> : <Copy size={16} />}
              <span>{copied ? t('profile.apiTokens.copied') : t('profile.apiTokens.copy')}</span>
            </Button>
            <Button type="button" variant="primary" onClick={() => setCreated(null)}>
              {t('profile.apiTokens.done')}
            </Button>
          </div>
        </div>
      )}

      {loadError && <span className={userStyles.error}>{loadError}</span>}
      {!loadError && tokens.length === 0 && (
        <span className={userStyles.hint}>{t('profile.apiTokens.empty')}</span>
      )}
      {tokens.length > 0 && (
        <ul className={styles.list}>
          {tokens.map((token) => (
            <li key={token.id} className={styles.item}>
              <div className={styles.itemMain}>
                <span className={styles.itemName}>{token.name}</span>
                <code className={styles.prefix}>{token.prefix}…</code>
                <span className={styles.role}>{roleName(token.role)}</span>
              </div>
              <div className={styles.itemMeta}>
                <span>
                  {token.expires_at
                    ? t('profile.apiTokens.expiresOn', { date: formatDate(token.expires_at) })
                    : t('profile.apiTokens.noExpiry')}
                </span>
                <span>
                  {token.last_used_at
                    ? t('profile.apiTokens.lastUsed', { date: formatDate(token.last_used_at) })
                    : t('profile.apiTokens.neverUsed')}
                </span>
              </div>
              <Button
                type="button"
                variant="secondary"
                danger
                onClick={() => handleRevoke(token)}
                aria-label={t('profile.apiTokens.revokeAria', { name: token.name })}
              >
                <Trash2 size={14} />
                <span>{t('profile.apiTokens.revoke')}</span>
              </Button>
            </li>
          ))}
        </ul>
      )}

      <form className={styles.form} onSubmit={handleCreate}>
        <Field label={t('profile.apiTokens.name')}>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t('profile.apiTokens.namePlaceholder')}
            maxLength={64}
            required
          />
        </Field>
        <div className={styles.row}>
          <Field label={t('profile.apiTokens.role')}>
            <select
              value={role}
              onChange={(e) => setRole(e.target.value as ApiToken['role'])}
              aria-label={t('profile.apiTokens.role')}
            >
              {allowedRoles.map((r) => (
                <option key={r} value={r}>
                  {roleName(r)}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t('profile.apiTokens.expiry')}>
            <select
              value={expiryKey}
              onChange={(e) => setExpiryKey(e.target.value as ExpiryKey)}
              aria-label={t('profile.apiTokens.expiry')}
            >
              {EXPIRY_CHOICES.map((c) => (
                <option key={c.key} value={c.key}>
                  {t(`profile.apiTokens.${c.key}`)}
                </option>
              ))}
            </select>
          </Field>
        </div>
        {createError && <span className={userStyles.error}>{createError}</span>}
        <div className={userStyles.actions}>
          <Button type="submit" variant="secondary" disabled={isCreating || !name.trim()}>
            <KeyRound size={16} />
            <span>
              {isCreating ? t('profile.apiTokens.creating') : t('profile.apiTokens.create')}
            </span>
          </Button>
        </div>
      </form>
    </div>
  );
};
