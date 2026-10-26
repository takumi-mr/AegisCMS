import React, { useEffect, useRef, useState } from 'react';
import { KeyRound, Save, UserRound } from 'lucide-react';
import { Trans, useTranslation } from 'react-i18next';
import { useRoleName } from '../i18n/roles';
import { aegisApi } from '../api/client';
import type { UserAccount } from '../types';
import { AvatarPicker } from './AvatarPicker';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import { Modal } from './ui/Modal';
import styles from './UserForms.module.css';

// Same minimum as the server (`auth::password::MIN_PASSWORD_LEN`)
export const MIN_PASSWORD_LENGTH = 8;

interface ProfileModalProps {
  user: UserAccount;
  /** False for single sign-on sessions: the password is managed by the identity provider */
  canChangePassword: boolean;
  /** Called whenever the server returns an updated account (profile, avatar) */
  onUpdated: (user: UserAccount) => void;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
  onClose: () => void;
}

/** The logged-in user's own profile: avatar, display name, email and password */
export const ProfileModal: React.FC<ProfileModalProps> = ({
  user,
  canChangePassword,
  onUpdated,
  onShowToast,
  onClose,
}) => {
  const { t } = useTranslation();
  const roleName = useRoleName();
  const [displayName, setDisplayName] = useState<string>(user.display_name ?? '');
  const [email, setEmail] = useState<string>(user.email ?? '');
  const [isSaving, setIsSaving] = useState<boolean>(false);
  const [profileError, setProfileError] = useState<string | null>(null);

  const [currentPassword, setCurrentPassword] = useState<string>('');
  const [newPassword, setNewPassword] = useState<string>('');
  const [confirmPassword, setConfirmPassword] = useState<string>('');
  const [isChangingPassword, setIsChangingPassword] = useState<boolean>(false);
  const [passwordError, setPasswordError] = useState<string | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    nameRef.current?.focus();
  }, []);

  const profileChanged =
    displayName.trim() !== (user.display_name ?? '') || email.trim() !== (user.email ?? '');

  const handleSaveProfile = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsSaving(true);
    setProfileError(null);
    try {
      const updated = await aegisApi.updateUser(user.username, {
        display_name: displayName.trim(),
        email: email.trim(),
      });
      onUpdated(updated);
      onShowToast(t('profile.updated'));
    } catch (err) {
      setProfileError(err instanceof Error ? err.message : t('profile.updateFailed'));
    } finally {
      setIsSaving(false);
    }
  };

  const handleChangePassword = async (e: React.FormEvent) => {
    e.preventDefault();
    if (newPassword.length < MIN_PASSWORD_LENGTH) {
      setPasswordError(t('profile.tooShort', { count: MIN_PASSWORD_LENGTH }));
      return;
    }
    if (newPassword !== confirmPassword) {
      setPasswordError(t('profile.mismatch'));
      return;
    }
    setIsChangingPassword(true);
    setPasswordError(null);
    try {
      await aegisApi.updateUser(user.username, {
        password: newPassword,
        current_password: currentPassword,
      });
      setCurrentPassword('');
      setNewPassword('');
      setConfirmPassword('');
      onShowToast(t('profile.changed'));
    } catch (err) {
      setPasswordError(err instanceof Error ? err.message : t('profile.changeFailed'));
    } finally {
      setIsChangingPassword(false);
    }
  };

  return (
    <Modal title={t('profile.title')} icon={<UserRound size={18} />} onClose={onClose} flush>
      <div className={styles.body}>
        <div className={styles.section}>
          <div className={styles.identity}>
            <Trans
              i18nKey="profile.signedInAs"
              values={{ username: user.username, role: roleName(user.role) }}
              components={{ code: <code /> }}
            />
          </div>
          <AvatarPicker user={user} onChanged={onUpdated} />
        </div>

        <form className={styles.section} onSubmit={handleSaveProfile}>
          <div className={styles.sectionTitle}>{t('profile.section')}</div>
          <Field label={t('profile.displayName')}>
            <input
              ref={nameRef}
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder={user.username}
              maxLength={100}
              aria-label={t('profile.displayNameAria')}
            />
          </Field>
          <Field label={t('profile.email')}>
            <input
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder={t('profile.optional')}
              aria-label={t('profile.emailAria')}
            />
          </Field>
          <span className={styles.hint}>{t('profile.nameHint')}</span>
          {profileError && <span className={styles.error}>{profileError}</span>}
          <div className={styles.actions}>
            <Button type="submit" variant="primary" disabled={isSaving || !profileChanged}>
              <Save size={16} />
              <span>{isSaving ? t('profile.saving') : t('profile.save')}</span>
            </Button>
          </div>
        </form>

        {!canChangePassword ? (
          <div className={styles.section}>
            <div className={styles.sectionTitle}>{t('profile.password')}</div>
            <span className={styles.hint}>{t('profile.ssoHint')}</span>
          </div>
        ) : (
          <form className={styles.section} onSubmit={handleChangePassword}>
            <div className={styles.sectionTitle}>{t('profile.changePassword')}</div>
            <Field label={t('profile.current')}>
              <input
                type="password"
                value={currentPassword}
                onChange={(e) => setCurrentPassword(e.target.value)}
                autoComplete="current-password"
                aria-label={t('profile.currentAria')}
              />
            </Field>
            <Field label={t('profile.new')}>
              <input
                type="password"
                value={newPassword}
                onChange={(e) => setNewPassword(e.target.value)}
                autoComplete="new-password"
                placeholder={t('profile.newPlaceholder', { count: MIN_PASSWORD_LENGTH })}
                aria-label={t('profile.newAria')}
              />
            </Field>
            <Field label={t('profile.confirm')}>
              <input
                type="password"
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                autoComplete="new-password"
                aria-label={t('profile.confirmAria')}
              />
            </Field>
            {passwordError && <span className={styles.error}>{passwordError}</span>}
            <div className={styles.actions}>
              <Button
                type="submit"
                variant="secondary"
                disabled={isChangingPassword || !currentPassword || !newPassword}
              >
                <KeyRound size={16} />
                <span>
                  {isChangingPassword ? t('profile.changing') : t('profile.changePassword')}
                </span>
              </Button>
            </div>
          </form>
        )}
      </div>
    </Modal>
  );
};
