import React, { useEffect, useRef, useState } from 'react';
import { ArrowLeft, Loader2, MailCheck, UserPlus } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import { describeRegistrationError, MIN_PASSWORD_LENGTH } from '../utils/verifyEmail';
import { Button } from './ui/Button';
import styles from './LoginView.module.css';

interface RegisterFormProps {
  /** Back to the sign-in form */
  onBack: () => void;
}

/**
 * Account application shown in place of the sign-in form. The server emails a confirmation
 * link; the account is created when the link is opened (see `takeVerifyToken`)
 */
export const RegisterForm: React.FC<RegisterFormProps> = ({ onBack }) => {
  const { t } = useTranslation();
  const [username, setUsername] = useState<string>('');
  const [email, setEmail] = useState<string>('');
  const [displayName, setDisplayName] = useState<string>('');
  const [password, setPassword] = useState<string>('');
  const [confirm, setConfirm] = useState<string>('');
  const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [sentTo, setSentTo] = useState<{ email: string; hours: number } | null>(null);
  const usernameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    usernameRef.current?.focus();
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!username.trim() || !email.trim() || !password) {
      setError(t('login.register.errors.missing'));
      return;
    }
    if (password.length < MIN_PASSWORD_LENGTH) {
      setError(t('login.register.errors.password_too_short', { count: MIN_PASSWORD_LENGTH }));
      return;
    }
    if (password !== confirm) {
      setError(t('login.register.errors.mismatch'));
      return;
    }
    setIsSubmitting(true);
    setError(null);
    try {
      const { expires_in_secs } = await aegisApi.register({
        username: username.trim(),
        email: email.trim(),
        password,
        ...(displayName.trim() ? { display_name: displayName.trim() } : {}),
      });
      setSentTo({ email: email.trim(), hours: Math.max(1, Math.ceil(expires_in_secs / 3600)) });
    } catch (err) {
      setError(describeRegistrationError(err, t));
    } finally {
      setIsSubmitting(false);
    }
  };

  if (sentTo) {
    return (
      <div className={styles.sent}>
        <MailCheck size={28} className={styles.sentIcon} />
        <h2>{t('login.register.sentTitle')}</h2>
        <p>{t('login.register.sent', { email: sentTo.email, count: sentTo.hours })}</p>
        <Button type="button" variant="secondary" onClick={onBack}>
          <ArrowLeft size={16} />
          <span>{t('login.register.back')}</span>
        </Button>
      </div>
    );
  }

  return (
    <form className={styles.form} onSubmit={handleSubmit}>
      <h2 className={styles.formTitle}>{t('login.register.title')}</h2>
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
          maxLength={64}
        />
      </label>
      <label className={styles.field}>
        <span>{t('login.register.email')}</span>
        <input
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          autoComplete="email"
          name="email"
        />
      </label>
      <label className={styles.field}>
        <span>
          {t('login.register.displayName')}{' '}
          <small className={styles.optional}>{t('login.register.optional')}</small>
        </span>
        <input
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          autoComplete="name"
          name="display_name"
        />
      </label>
      <label className={styles.field}>
        <span>{t('login.password')}</span>
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="new-password"
          name="password"
        />
      </label>
      <label className={styles.field}>
        <span>{t('login.register.confirmPassword')}</span>
        <input
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          autoComplete="new-password"
          name="confirm_password"
        />
      </label>
      <Button type="submit" variant="primary" className={styles.submit} disabled={isSubmitting}>
        {isSubmitting ? <Loader2 size={16} className="animate-spin" /> : <UserPlus size={16} />}
        <span>{isSubmitting ? t('login.register.submitting') : t('login.register.submit')}</span>
      </Button>
      <button type="button" className={styles.linkButton} onClick={onBack}>
        {t('login.register.haveAccount')}
      </button>
    </form>
  );
};
