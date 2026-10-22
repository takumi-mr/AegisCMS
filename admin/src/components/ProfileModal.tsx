import React, { useEffect, useRef, useState } from 'react';
import { KeyRound, Save, UserRound } from 'lucide-react';
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
      onShowToast('Profile updated');
    } catch (err) {
      setProfileError(err instanceof Error ? err.message : 'Failed to update the profile');
    } finally {
      setIsSaving(false);
    }
  };

  const handleChangePassword = async (e: React.FormEvent) => {
    e.preventDefault();
    if (newPassword.length < MIN_PASSWORD_LENGTH) {
      setPasswordError(`The new password must be at least ${MIN_PASSWORD_LENGTH} characters.`);
      return;
    }
    if (newPassword !== confirmPassword) {
      setPasswordError('The new passwords do not match.');
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
      onShowToast('Password changed');
    } catch (err) {
      setPasswordError(err instanceof Error ? err.message : 'Failed to change the password');
    } finally {
      setIsChangingPassword(false);
    }
  };

  return (
    <Modal title="My profile" icon={<UserRound size={18} />} onClose={onClose}>
      <div className={styles.body}>
        <div className={styles.section}>
          <div className={styles.identity}>
            Signed in as <code>{user.username}</code> ({user.role})
          </div>
          <AvatarPicker user={user} onChanged={onUpdated} />
        </div>

        <form className={styles.section} onSubmit={handleSaveProfile}>
          <div className={styles.sectionTitle}>Profile</div>
          <Field label="display name:">
            <input
              ref={nameRef}
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder={user.username}
              maxLength={100}
              aria-label="Display name"
            />
          </Field>
          <Field label="email:">
            <input
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="Optional"
              aria-label="Email"
            />
          </Field>
          <span className={styles.hint}>
            The header shows the new name right away; other sessions pick it up at the next sign-in.
          </span>
          {profileError && <span className={styles.error}>{profileError}</span>}
          <div className={styles.actions}>
            <Button type="submit" variant="primary" disabled={isSaving || !profileChanged}>
              <Save size={16} />
              <span>{isSaving ? 'Saving...' : 'Save profile'}</span>
            </Button>
          </div>
        </form>

        {!canChangePassword ? (
          <div className={styles.section}>
            <div className={styles.sectionTitle}>Password</div>
            <span className={styles.hint}>
              You signed in with single sign-on. Change your password with your identity provider.
            </span>
          </div>
        ) : (
          <form className={styles.section} onSubmit={handleChangePassword}>
            <div className={styles.sectionTitle}>Change password</div>
            <Field label="current:">
              <input
                type="password"
                value={currentPassword}
                onChange={(e) => setCurrentPassword(e.target.value)}
                autoComplete="current-password"
                aria-label="Current password"
              />
            </Field>
            <Field label="new:">
              <input
                type="password"
                value={newPassword}
                onChange={(e) => setNewPassword(e.target.value)}
                autoComplete="new-password"
                placeholder={`At least ${MIN_PASSWORD_LENGTH} characters`}
                aria-label="New password"
              />
            </Field>
            <Field label="confirm:">
              <input
                type="password"
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                autoComplete="new-password"
                aria-label="Confirm new password"
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
                <span>{isChangingPassword ? 'Changing...' : 'Change password'}</span>
              </Button>
            </div>
          </form>
        )}
      </div>
    </Modal>
  );
};
