import React, { useEffect, useRef, useState } from 'react';
import { KeyRound, Loader2, LogIn, UserPlus } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { aegisApi, ApiError, SERVER_URL } from '../api/client';
import type { AuthProviders, SessionInfo, SsoProvider } from '../types';
import { BrandMark } from './ui/BrandMark';
import { Button } from './ui/Button';
import { LanguageSelect } from './ui/LanguageSelect';
import { isSsoErrorCode } from '../utils/loginRedirect';
import { describeRegistrationError } from '../utils/verifyEmail';
import { RegisterForm } from './RegisterForm';
import { PasswordResetRequestForm, PasswordResetConfirmForm } from './PasswordResetForms';
import { AcceptInviteForm } from './AcceptInviteForm';
import styles from './LoginView.module.css';

interface LoginViewProps {
  onLoggedIn: (session: SessionInfo) => void;
  /** Shown above the form, e.g. after the previous session expired */
  notice?: string | null;
  /** Error to show first: a failed single sign-on (a reason code or a plugin's message) */
  initialError?: string | null;
  /** Token of an email confirmation link (`#verify-email=`), confirmed when the screen opens */
  verifyToken?: string | null;
  /** Token of a password reset link (`#reset-password=`) */
  resetToken?: string | null;
  /** Token of an invitation link (`#invite=`) */
  inviteToken?: string | null;
}

type ScreenMode =
  'login' | 'register' | 'forgot_password' | 'reset_password' | 'accept_invite' | 'mfa';

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
export const LoginView: React.FC<LoginViewProps> = ({
  onLoggedIn,
  notice,
  initialError,
  verifyToken,
  resetToken,
  inviteToken,
}) => {
  const { t } = useTranslation();
  const [username, setUsername] = useState<string>('');
  const [password, setPassword] = useState<string>('');
  const [mfaChallengeToken, setMfaChallengeToken] = useState<string | null>(null);
  const [mfaCode, setMfaCode] = useState<string>('');
  const [useBackupCode, setUseBackupCode] = useState<boolean>(false);
  const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(() =>
    initialError ? describeSsoError(initialError, t) : null
  );
  const [providers, setProviders] = useState<AuthProviders | null>(null);
  const [mode, setMode] = useState<ScreenMode>(() => {
    if (resetToken) return 'reset_password';
    if (inviteToken) return 'accept_invite';
    return 'login';
  });
  const [verifyNotice, setVerifyNotice] = useState<string | null>(null);
  const [customNotice, setCustomNotice] = useState<string | null>(null);
  const [isVerifying, setIsVerifying] = useState<boolean>(Boolean(verifyToken));
  const usernameRef = useRef<HTMLInputElement>(null);
  const ssoProviders = [providers?.oidc, providers?.saml].filter((sso): sso is SsoProvider =>
    Boolean(sso)
  );

  useEffect(() => {
    if (mode === 'login') {
      usernameRef.current?.focus();
    }
    // Without the list (older server, offline) only the password form is shown
    aegisApi
      .getAuthProviders()
      .then(setProviders)
      .catch(() => setProviders(null));
  }, [mode]);

  useEffect(() => {
    if (!verifyToken) return;
    let cancelled = false;
    aegisApi
      .verifyRegistration(verifyToken)
      .then((result) => {
        if (cancelled) return;
        setUsername(result.username);
        setVerifyNotice(
          t(`login.register.verified.${result.status}`, { username: result.username })
        );
      })
      .catch((err) => {
        if (!cancelled) setError(describeRegistrationError(err, t));
      })
      .finally(() => {
        if (!cancelled) setIsVerifying(false);
      });
    return () => {
      cancelled = true;
    };
    // The token is read once from the URL; `t` is stable for the screen's lifetime
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [verifyToken]);

  const backToLogin = () => {
    setMode('login');
    setError(null);
    setMfaChallengeToken(null);
    setMfaCode('');
    setUseBackupCode(false);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!username.trim() || !password) {
      setError(t('login.errors.missing'));
      return;
    }
    setIsSubmitting(true);
    setError(null);
    try {
      const res = await aegisApi.login(username.trim(), password);
      if ('mfa_required' in res && res.mfa_required) {
        setMfaChallengeToken(res.challenge_token);
        setMode('mfa');
        setError(null);
        return;
      }
      onLoggedIn(res as SessionInfo);
    } catch (err) {
      setError(describeError(err, t));
      setPassword('');
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleMfaSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!mfaCode.trim() || !mfaChallengeToken) return;
    setIsSubmitting(true);
    setError(null);
    try {
      const session = await aegisApi.verify2fa(mfaChallengeToken, mfaCode.trim());
      onLoggedIn(session);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : t('login.twoFactor.invalidCode'));
      setMfaCode('');
    } finally {
      setIsSubmitting(false);
    }
  };

  const getSubtitle = () => {
    if (mode === 'register') return t('login.register.subtitle');
    if (mode === 'forgot_password' || mode === 'reset_password') {
      return t('login.resetPassword.requestSubtitle');
    }
    if (mode === 'accept_invite') return t('login.invitation.subtitle');
    if (mode === 'mfa') return t('login.twoFactor.title');
    return t('login.subtitle');
  };

  const brand = (
    <div className={styles.brand}>
      <BrandMark />
      <div>
        <h1>AegisCMS</h1>
        <p>{getSubtitle()}</p>
      </div>
    </div>
  );

  if (mode === 'mfa') {
    return (
      <div className={styles.screen}>
        <div className={styles.card}>
          {brand}
          <form className={styles.form} onSubmit={handleMfaSubmit}>
            <div className={styles.mfaHeader}>
              <KeyRound size={28} className={styles.mfaIcon} />
              <h2>{t('login.twoFactor.title')}</h2>
              <p className={styles.mfaSubtitle}>{t('login.twoFactor.subtitle')}</p>
            </div>
            <div className={styles.field}>
              <label htmlFor="mfa-code">
                {useBackupCode
                  ? t('login.twoFactor.backupCodeLabel')
                  : t('login.twoFactor.codeLabel')}
              </label>
              <input
                id="mfa-code"
                type="text"
                value={mfaCode}
                onChange={(e) => setMfaCode(e.target.value)}
                placeholder={
                  useBackupCode
                    ? t('login.twoFactor.backupCodePlaceholder')
                    : t('login.twoFactor.codePlaceholder')
                }
                autoFocus
                autoComplete="one-time-code"
                required
              />
            </div>
            {error && <div className={styles.error}>{error}</div>}
            <div className={styles.submit}>
              <Button type="submit" variant="primary" disabled={isSubmitting || !mfaCode.trim()}>
                {isSubmitting ? (
                  <>
                    <Loader2 size={16} className={styles.spinner} />
                    <span>{t('login.twoFactor.verifying')}</span>
                  </>
                ) : (
                  <>
                    <LogIn size={16} />
                    <span>{t('login.twoFactor.verify')}</span>
                  </>
                )}
              </Button>
            </div>
            <div className={styles.links}>
              <button
                type="button"
                className={styles.linkButton}
                onClick={() => {
                  setUseBackupCode(!useBackupCode);
                  setMfaCode('');
                  setError(null);
                }}
              >
                {useBackupCode
                  ? t('login.twoFactor.useTotpCode')
                  : t('login.twoFactor.useBackupCode')}
              </button>
              <button type="button" className={styles.linkButton} onClick={backToLogin}>
                {t('login.twoFactor.backToLogin')}
              </button>
            </div>
          </form>
          <LanguageSelect className={styles.language} />
        </div>
      </div>
    );
  }

  if (mode === 'register') {
    return (
      <div className={styles.screen}>
        <div className={styles.card}>
          {brand}
          <RegisterForm onBack={backToLogin} />
          <LanguageSelect className={styles.language} />
        </div>
      </div>
    );
  }

  if (mode === 'forgot_password') {
    return (
      <div className={styles.screen}>
        <div className={styles.card}>
          {brand}
          <PasswordResetRequestForm onBack={backToLogin} />
          <LanguageSelect className={styles.language} />
        </div>
      </div>
    );
  }

  if (mode === 'reset_password' && resetToken) {
    return (
      <div className={styles.screen}>
        <div className={styles.card}>
          {brand}
          <PasswordResetConfirmForm
            token={resetToken}
            onSuccess={(msg) => {
              setCustomNotice(msg);
              setMode('login');
            }}
            onBack={backToLogin}
          />
          <LanguageSelect className={styles.language} />
        </div>
      </div>
    );
  }

  if (mode === 'accept_invite' && inviteToken) {
    return (
      <div className={styles.screen}>
        <div className={styles.card}>
          {brand}
          <AcceptInviteForm
            token={inviteToken}
            onSuccess={(msg) => {
              setCustomNotice(msg);
              setMode('login');
            }}
            onBack={backToLogin}
          />
          <LanguageSelect className={styles.language} />
        </div>
      </div>
    );
  }

  const shownNotice = isVerifying
    ? t('login.register.verifying')
    : (customNotice ?? verifyNotice ?? notice);

  const showPasswordReset = providers?.password_reset !== false;

  return (
    <div className={styles.screen}>
      <form className={styles.card} onSubmit={handleSubmit}>
        {brand}

        {shownNotice && !error && (
          <div className={styles.notice} role="status">
            {shownNotice}
          </div>
        )}
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

        {showPasswordReset && (
          <button
            type="button"
            className={styles.forgotButton}
            onClick={() => {
              setMode('forgot_password');
              setError(null);
            }}
          >
            {t('login.forgotPassword')}
          </button>
        )}

        <Button type="submit" variant="primary" className={styles.submit} disabled={isSubmitting}>
          {isSubmitting ? <Loader2 size={16} className="animate-spin" /> : <LogIn size={16} />}
          <span>{isSubmitting ? t('login.submitting') : t('login.submit')}</span>
        </Button>

        {ssoProviders.length > 0 && (
          <>
            <div className={styles.divider}>
              <span>{t('login.or')}</span>
            </div>
            {ssoProviders.map((sso) => (
              <a key={sso.login_url} className={styles.sso} href={`${SERVER_URL}${sso.login_url}`}>
                <KeyRound size={16} />
                <span>{t('login.sso', { provider: sso.display_name })}</span>
              </a>
            ))}
          </>
        )}

        {providers?.registration && (
          <button
            type="button"
            className={styles.registerLink}
            onClick={() => {
              setMode('register');
              setError(null);
            }}
          >
            <UserPlus size={15} />
            <span>{t('login.register.link')}</span>
          </button>
        )}

        <LanguageSelect className={styles.language} />

        {!providers?.registration && (
          <p className={styles.hint}>
            {t('login.hint')} <code>aegis-server user add &lt;username&gt; --role admin</code>.
          </p>
        )}
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
