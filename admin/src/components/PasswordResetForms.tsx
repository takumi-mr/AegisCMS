import React, { useEffect, useRef, useState } from 'react';
import { ArrowLeft, CheckCircle2, KeyRound, Loader2, MailCheck } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import { MIN_PASSWORD_LENGTH } from '../utils/verifyEmail';
import { Button } from './ui/Button';
import styles from './LoginView.module.css';

interface PasswordResetRequestFormProps {
  onBack: () => void;
}

/** Form to request a password reset email */
export const PasswordResetRequestForm: React.FC<PasswordResetRequestFormProps> = ({ onBack }) => {
  const { t } = useTranslation();
  const [email, setEmail] = useState<string>('');
  const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [sentEmail, setSentEmail] = useState<string | null>(null);
  const emailRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    emailRef.current?.focus();
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!email.trim()) {
      setError(t('login.register.errors.invalid_email'));
      return;
    }
    setIsSubmitting(true);
    setError(null);
    try {
      await aegisApi.requestPasswordReset(email.trim());
      setSentEmail(email.trim());
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t('common.unreachable'));
    } finally {
      setIsSubmitting(false);
    }
  };

  if (sentEmail) {
    return (
      <div className={styles.sent}>
        <MailCheck size={28} className={styles.sentIcon} />
        <h2>{t('login.resetPassword.sentTitle')}</h2>
        <p>{t('login.resetPassword.sent', { email: sentEmail })}</p>
        <Button type="button" variant="secondary" onClick={onBack}>
          <ArrowLeft size={16} />
          <span>{t('login.resetPassword.back')}</span>
        </Button>
      </div>
    );
  }

  return (
    <form className={styles.form} onSubmit={handleSubmit}>
      <h2 className={styles.formTitle}>{t('login.resetPassword.requestTitle')}</h2>
      <p className={styles.hint}>{t('login.resetPassword.requestSubtitle')}</p>
      {error && (
        <div className={styles.error} role="alert">
          {error}
        </div>
      )}
      <label className={styles.field}>
        <span>{t('login.resetPassword.email')}</span>
        <input
          ref={emailRef}
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          autoComplete="email"
          required
        />
      </label>
      <Button type="submit" variant="primary" className={styles.submit} disabled={isSubmitting}>
        {isSubmitting ? <Loader2 size={16} className="animate-spin" /> : <KeyRound size={16} />}
        <span>
          {isSubmitting ? t('login.resetPassword.submitting') : t('login.resetPassword.submit')}
        </span>
      </Button>
      <button type="button" className={styles.linkButton} onClick={onBack}>
        {t('login.resetPassword.back')}
      </button>
    </form>
  );
};

interface PasswordResetConfirmFormProps {
  token: string;
  onSuccess: (message: string) => void;
  onBack: () => void;
}

/** Form to set a new password via reset token */
export const PasswordResetConfirmForm: React.FC<PasswordResetConfirmFormProps> = ({
  token,
  onSuccess,
  onBack,
}) => {
  const { t } = useTranslation();
  const [password, setPassword] = useState<string>('');
  const [confirm, setConfirm] = useState<string>('');
  const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const passwordRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    passwordRef.current?.focus();
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!password) {
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
      await aegisApi.confirmPasswordReset(token, password);
      onSuccess(t('login.resetPassword.success'));
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t('login.resetPassword.invalidToken'));
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <form className={styles.form} onSubmit={handleSubmit}>
      <h2 className={styles.formTitle}>{t('login.resetPassword.confirmTitle')}</h2>
      {error && (
        <div className={styles.error} role="alert">
          {error}
        </div>
      )}
      <label className={styles.field}>
        <span>{t('login.resetPassword.newPassword')}</span>
        <input
          ref={passwordRef}
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="new-password"
          required
        />
      </label>
      <label className={styles.field}>
        <span>{t('login.resetPassword.confirmPassword')}</span>
        <input
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          autoComplete="new-password"
          required
        />
      </label>
      <Button type="submit" variant="primary" className={styles.submit} disabled={isSubmitting}>
        {isSubmitting ? <Loader2 size={16} className="animate-spin" /> : <CheckCircle2 size={16} />}
        <span>
          {isSubmitting
            ? t('login.resetPassword.submittingConfirm')
            : t('login.resetPassword.submitConfirm')}
        </span>
      </Button>
      <button type="button" className={styles.linkButton} onClick={onBack}>
        {t('login.resetPassword.back')}
      </button>
    </form>
  );
};
