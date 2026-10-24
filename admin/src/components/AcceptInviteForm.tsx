import React, { useEffect, useRef, useState } from 'react';
import { Loader2, UserPlus } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type { InvitationInfo } from '../types';
import { MIN_PASSWORD_LENGTH } from '../utils/verifyEmail';
import { Button } from './ui/Button';
import { RoleLabel } from './ui/RoleLabel';
import styles from './LoginView.module.css';

interface AcceptInviteFormProps {
  token: string;
  onSuccess: (message: string) => void;
  onBack: () => void;
}

export const AcceptInviteForm: React.FC<AcceptInviteFormProps> = ({ token, onSuccess, onBack }) => {
  const { t } = useTranslation();
  const [info, setInfo] = useState<InvitationInfo | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [username, setUsername] = useState<string>('');
  const [displayName, setDisplayName] = useState<string>('');
  const [password, setPassword] = useState<string>('');
  const [confirm, setConfirm] = useState<string>('');
  const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const usernameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getInvitationInfo(token)
      .then((data) => {
        if (cancelled) return;
        setInfo(data);
        if (data.display_name) {
          setDisplayName(data.display_name);
        }
      })
      .catch(() => {
        if (cancelled) return;
        setError(t('login.invitation.invalid'));
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [token, t]);

  useEffect(() => {
    if (!isLoading && info) {
      usernameRef.current?.focus();
    }
  }, [isLoading, info]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!username.trim() || !password) {
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
      await aegisApi.acceptInvitation({
        token,
        username: username.trim(),
        password,
        ...(displayName.trim() ? { display_name: displayName.trim() } : {}),
      });
      onSuccess(t('login.invitation.success'));
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t('common.unreachable'));
    } finally {
      setIsSubmitting(false);
    }
  };

  if (isLoading) {
    return (
      <div className={styles.form}>
        <div className={styles.checking}>
          <Loader2 size={20} className="animate-spin" />
        </div>
      </div>
    );
  }

  if (!info || error) {
    return (
      <div className={styles.form}>
        <h2 className={styles.formTitle}>{t('login.invitation.title')}</h2>
        <div className={styles.error} role="alert">
          {error || t('login.invitation.invalid')}
        </div>
        <button type="button" className={styles.linkButton} onClick={onBack}>
          {t('login.resetPassword.back')}
        </button>
      </div>
    );
  }

  return (
    <form className={styles.form} onSubmit={handleSubmit}>
      <h2 className={styles.formTitle}>{t('login.invitation.title')}</h2>
      <p className={styles.hint}>{t('login.invitation.subtitle')}</p>

      {error && (
        <div className={styles.error} role="alert">
          {error}
        </div>
      )}

      <label className={styles.field}>
        <span>{t('login.invitation.invitedEmail')}</span>
        <input value={info.email} disabled readOnly />
      </label>

      <div className={styles.field}>
        <span>{t('login.invitation.role')}</span>
        <div>
          <RoleLabel role={info.role} />
        </div>
      </div>

      <label className={styles.field}>
        <span>{t('login.username')}</span>
        <input
          ref={usernameRef}
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          autoComplete="username"
          required
          maxLength={64}
        />
      </label>

      <label className={styles.field}>
        <span>
          {t('login.invitation.displayName')}{' '}
          <small className={styles.optional}>{t('login.register.optional')}</small>
        </span>
        <input
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          autoComplete="name"
        />
      </label>

      <label className={styles.field}>
        <span>{t('login.password')}</span>
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="new-password"
          required
        />
      </label>

      <label className={styles.field}>
        <span>{t('login.invitation.confirmPassword')}</span>
        <input
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          autoComplete="new-password"
          required
        />
      </label>

      <Button type="submit" variant="primary" className={styles.submit} disabled={isSubmitting}>
        {isSubmitting ? <Loader2 size={16} className="animate-spin" /> : <UserPlus size={16} />}
        <span>
          {isSubmitting ? t('login.invitation.submitting') : t('login.invitation.submit')}
        </span>
      </Button>

      <button type="button" className={styles.linkButton} onClick={onBack}>
        {t('login.resetPassword.back')}
      </button>
    </form>
  );
};
