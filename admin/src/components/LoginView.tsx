import React, { useEffect, useRef, useState } from 'react';
import { KeyRound, Loader2, LogIn } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { aegisApi, ApiError, SERVER_URL } from '../api/client';
import type { AuthProviders, SessionInfo } from '../types';
import { BrandMark } from './ui/BrandMark';
import { Button } from './ui/Button';
import { LanguageSelect } from './ui/LanguageSelect';
import { isSsoErrorCode } from '../utils/loginRedirect';
import styles from './LoginView.module.css';

interface LoginViewProps {
  onLoggedIn: (session: SessionInfo) => void;
  /** Shown above the form, e.g. after the previous session expired */
  notice?: string | null;
  /** Error to show first: a failed single sign-on (a reason code or a plugin's message) */
  initialError?: string | null;
}

const describeError = (err: unknown, t: TFunction): string => {
  if (err instanceof ApiError) {
    if (err.status === 401) return t('login.errors.invalid');
    if (err.status === 429) {
      const minutes = Math.max(1, Math.ceil((err.retryAfterSecs ?? 60) / 60));
      return t('login.errors.locked', { count: minutes });
    }
    return err.message;
  }
  // fetch rejects with a TypeError when the server cannot be reached
  return t('common.unreachable');
};

const describeSsoError = (error: string, t: TFunction): string =>
  isSsoErrorCode(error) ? t(`login.sso_errors.${error}`) : error;

/** Full-screen username / password form shown until the user has a valid session */
export const LoginView: React.FC<LoginViewProps> = ({ onLoggedIn, notice, initialError }) => {
  const { t } = useTranslation();
  const [username, setUsername] = useState<string>('');
  const [password, setPassword] = useState<string>('');
  const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(() =>
    initialError ? describeSsoError(initialError, t) : null
  );
  const [providers, setProviders] = useState<AuthProviders | null>(null);
  const usernameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    usernameRef.current?.focus();
    // Without the list (older server, offline) only the password form is shown
    aegisApi
      .getAuthProviders()
      .then(setProviders)
      .catch(() => setProviders(null));
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!username.trim() || !password) {
      setError(t('login.errors.missing'));
      return;
    }
    setIsSubmitting(true);
    setError(null);
    try {
      onLoggedIn(await aegisApi.login(username.trim(), password));
    } catch (err) {
      setError(describeError(err, t));
      setPassword('');
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className={styles.screen}>
      <form className={styles.card} onSubmit={handleSubmit}>
        <div className={styles.brand}>
          <BrandMark />
          <div>
            <h1>AegisCMS</h1>
            <p>{t('login.subtitle')}</p>
          </div>
        </div>

        {notice && !error && <div className={styles.notice}>{notice}</div>}
        {error && (
          <div className={styles.error} role="alert">
            {error}
          </div>
        )}

        <label className={styles.field}>
          <span>{t('login.username')}</span>
          <input
            ref={usernameRef}
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            autoComplete="username"
            name="username"
          />
        </label>
        <label className={styles.field}>
          <span>{t('login.password')}</span>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="current-password"
            name="password"
          />
        </label>

        <Button type="submit" variant="primary" className={styles.submit} disabled={isSubmitting}>
          {isSubmitting ? <Loader2 size={16} className="animate-spin" /> : <LogIn size={16} />}
          <span>{isSubmitting ? t('login.submitting') : t('login.submit')}</span>
        </Button>

        {providers?.oidc && (
          <>
            <div className={styles.divider}>
              <span>{t('login.or')}</span>
            </div>
            <a className={styles.sso} href={`${SERVER_URL}${providers.oidc.login_url}`}>
              <KeyRound size={16} />
              <span>{t('login.sso', { provider: providers.oidc.display_name })}</span>
            </a>
          </>
        )}

        <LanguageSelect className={styles.language} />

        <p className={styles.hint}>
          {t('login.hint')} <code>aegis-server user add &lt;username&gt; --role admin</code>.
        </p>
      </form>
    </div>
  );
};

/** Placeholder shown while the stored session token is being checked */
export const SessionCheckingScreen: React.FC = () => {
  const { t } = useTranslation();
  return (
    <div className={styles.screen}>
      <span className={styles.checking}>{t('login.checking')}</span>
    </div>
  );
};
