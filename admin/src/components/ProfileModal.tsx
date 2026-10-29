import React, { useEffect, useRef, useState } from 'react';
import { Check, Copy, KeyRound, Save, ShieldAlert, ShieldCheck, UserRound } from 'lucide-react';
import type { TotpSetupResult } from '../types';
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

  // 2FA state
  const [twoFactorEnabled, setTwoFactorEnabled] = useState<boolean>(Boolean(user.totp_enabled));
  const [setupData, setSetupData] = useState<TotpSetupResult | null>(null);
  const [setupCode, setSetupCode] = useState<string>('');
  const [backupCodes, setBackupCodes] = useState<string[] | null>(null);
  const [twoFactorError, setTwoFactorError] = useState<string | null>(null);
  const [isSettingUp2fa, setIsSettingUp2fa] = useState<boolean>(false);
  const [isVerifying2fa, setIsVerifying2fa] = useState<boolean>(false);
  const [showDisablePrompt, setShowDisablePrompt] = useState<boolean>(false);
  const [disablePassword, setDisablePassword] = useState<string>('');
  const [isDisabling2fa, setIsDisabling2fa] = useState<boolean>(false);
  const [copiedBackupCodes, setCopiedBackupCodes] = useState<boolean>(false);
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

  const handleStart2faSetup = async () => {
    setIsSettingUp2fa(true);
    setTwoFactorError(null);
    try {
      const data = await aegisApi.setup2fa();
      setSetupData(data);
    } catch (err) {
      setTwoFactorError(err instanceof Error ? err.message : 'Failed to set up 2FA');
    } finally {
      setIsSettingUp2fa(false);
    }
  };

  const handleConfirm2fa = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!setupData || !setupCode.trim()) return;
    setIsVerifying2fa(true);
    setTwoFactorError(null);
    try {
      const res = await aegisApi.enable2fa(setupData.secret, setupCode.trim());
      setBackupCodes(res.backup_codes);
      setTwoFactorEnabled(true);
      setSetupData(null);
      setSetupCode('');
      onUpdated({ ...user, totp_enabled: true });
      onShowToast(t('profile.twoFactor.enabledSuccess'));
    } catch (err) {
      setTwoFactorError(err instanceof Error ? err.message : 'Invalid code');
    } finally {
      setIsVerifying2fa(false);
    }
  };

  const handleDisable2fa = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsDisabling2fa(true);
    setTwoFactorError(null);
    try {
      await aegisApi.disable2fa({ password: disablePassword });
      setTwoFactorEnabled(false);
      setShowDisablePrompt(false);
      setDisablePassword('');
      onUpdated({ ...user, totp_enabled: false });
      onShowToast(t('profile.twoFactor.disabledSuccess'));
    } catch (err) {
      setTwoFactorError(err instanceof Error ? err.message : 'Failed to disable 2FA');
    } finally {
      setIsDisabling2fa(false);
    }
  };

  const handleCopyBackupCodes = () => {
    if (!backupCodes) return;
    navigator.clipboard.writeText(backupCodes.join('\n')).then(() => {
      setCopiedBackupCodes(true);
      setTimeout(() => setCopiedBackupCodes(false), 2000);
    });
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

        <div className={styles.section}>
          <div className={styles.sectionTitle}>{t('profile.twoFactor.title')}</div>
          <div className={styles.twoFactorStatus}>
            <span
              className={`${styles.twoFactorBadge} ${
                twoFactorEnabled ? styles.twoFactorBadgeOn : styles.twoFactorBadgeOff
              }`}
            >
              {twoFactorEnabled ? <ShieldCheck size={15} /> : <ShieldAlert size={15} />}
              <span>
                {twoFactorEnabled
                  ? t('profile.twoFactor.statusEnabled')
                  : t('profile.twoFactor.statusDisabled')}
              </span>
            </span>
            {!twoFactorEnabled && !setupData && !backupCodes && (
              <Button
                type="button"
                variant="secondary"
                onClick={handleStart2faSetup}
                disabled={isSettingUp2fa}
              >
                {t('profile.twoFactor.setupBtn')}
              </Button>
            )}
            {twoFactorEnabled && !showDisablePrompt && (
              <Button type="button" variant="secondary" onClick={() => setShowDisablePrompt(true)}>
                {t('profile.twoFactor.disableBtn')}
              </Button>
            )}
          </div>

          {backupCodes && (
            <div className={styles.backupContainer}>
              <div className={styles.backupWarning}>{t('profile.twoFactor.backupWarning')}</div>
              <div className={styles.backupGrid}>
                {backupCodes.map((code, idx) => (
                  <div key={idx} className={styles.backupItem}>
                    {code}
                  </div>
                ))}
              </div>
              <div className={styles.actions}>
                <Button type="button" variant="secondary" onClick={handleCopyBackupCodes}>
                  {copiedBackupCodes ? <Check size={16} /> : <Copy size={16} />}
                  <span>
                    {copiedBackupCodes
                      ? t('profile.twoFactor.copied')
                      : t('profile.twoFactor.copyAll')}
                  </span>
                </Button>
                <Button type="button" variant="primary" onClick={() => setBackupCodes(null)}>
                  {t('profile.twoFactor.complete')}
                </Button>
              </div>
            </div>
          )}

          {setupData && !backupCodes && (
            <form className={styles.twoFactorBox} onSubmit={handleConfirm2fa}>
              <p className={styles.hint}>{t('profile.twoFactor.scanQr')}</p>
              <div className={styles.qrContainer}>
                <img
                  src={`data:image/png;base64,${setupData.qr_code_base64}`}
                  alt="2FA QR Code"
                  className={styles.qrImage}
                />
              </div>
              <div className={styles.secretKey}>
                <span className={styles.hint}>{t('profile.twoFactor.manualSecret')} </span>
                <code>{setupData.secret}</code>
              </div>
              <Field label={t('profile.twoFactor.enterCode')}>
                <input
                  type="text"
                  value={setupCode}
                  onChange={(e) => setSetupCode(e.target.value)}
                  placeholder="123456"
                  maxLength={10}
                  autoComplete="one-time-code"
                  required
                />
              </Field>
              {twoFactorError && <span className={styles.error}>{twoFactorError}</span>}
              <div className={styles.actions}>
                <Button
                  type="button"
                  variant="secondary"
                  onClick={() => {
                    setSetupData(null);
                    setSetupCode('');
                    setTwoFactorError(null);
                  }}
                >
                  {t('common.cancel')}
                </Button>
                <Button
                  type="submit"
                  variant="primary"
                  disabled={isVerifying2fa || !setupCode.trim()}
                >
                  {isVerifying2fa
                    ? t('profile.twoFactor.verifying')
                    : t('profile.twoFactor.verifyAndEnable')}
                </Button>
              </div>
            </form>
          )}

          {showDisablePrompt && (
            <form className={styles.twoFactorBox} onSubmit={handleDisable2fa}>
              <p className={styles.hint}>{t('profile.twoFactor.confirmDisablePrompt')}</p>
              <Field label={t('profile.current')}>
                <input
                  type="password"
                  value={disablePassword}
                  onChange={(e) => setDisablePassword(e.target.value)}
                  autoComplete="current-password"
                  required
                />
              </Field>
              {twoFactorError && <span className={styles.error}>{twoFactorError}</span>}
              <div className={styles.actions}>
                <Button
                  type="button"
                  variant="secondary"
                  onClick={() => {
                    setShowDisablePrompt(false);
                    setDisablePassword('');
                    setTwoFactorError(null);
                  }}
                >
                  {t('common.cancel')}
                </Button>
                <Button
                  type="submit"
                  variant="secondary"
                  danger
                  disabled={isDisabling2fa || !disablePassword}
                >
                  {isDisabling2fa
                    ? t('profile.twoFactor.disabling')
                    : t('profile.twoFactor.disableConfirmBtn')}
                </Button>
              </div>
            </form>
          )}
        </div>
      </div>
    </Modal>
  );
};
