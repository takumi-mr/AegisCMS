import React, { useEffect, useRef, useState } from 'react';
import { CheckCircle2, Mail, Pencil, Plus, RefreshCw, Trash2, UserPlus, Users } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, ApiError } from '../api/client';
import i18n from '../i18n';
import type { SessionInfo, UserAccount, UserRole } from '../types';
import { cx } from '../utils/cx';
import { formatDate } from '../utils/format';
import { AvatarPicker } from './AvatarPicker';
import { MIN_PASSWORD_LENGTH } from './ProfileModal';
import { Avatar } from './ui/Avatar';
import { Button } from './ui/Button';
import { Field, FieldRow } from './ui/Field';
import { Modal } from './ui/Modal';
import { RoleLabel } from './ui/RoleLabel';
import styles from './UserManager.module.css';
import formStyles from './UserForms.module.css';

const ROLES: UserRole[] = ['admin', 'editor', 'viewer'];

const ROLE_BADGE: Record<string, string | undefined> = {
  admin: styles.roleAdmin,
  editor: styles.roleEditor,
};

const messageOf = (err: unknown, fallback: string): string =>
  err instanceof Error ? err.message : fallback;

interface UserManagerProps {
  session: SessionInfo;
  /** The signed-in user's account changed (e.g. an admin edited their own avatar here) */
  onSelfUpdated: (user: UserAccount) => void;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/** Admin screen listing every account, with create, edit, avatar and delete */
export const UserManager: React.FC<UserManagerProps> = ({
  session,
  onSelfUpdated,
  onShowToast,
}) => {
  const { t } = useTranslation();
  const [users, setUsers] = useState<UserAccount[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Loading = the last finished request is older than the latest reload
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [loadedCount, setLoadedCount] = useState<number>(-1);
  const isLoading = loadedCount !== reloadCount;
  const [editing, setEditing] = useState<UserAccount | null>(null);
  const [isCreating, setIsCreating] = useState<boolean>(false);
  const [isInviting, setIsInviting] = useState<boolean>(false);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listUsers()
      .then((loaded) => {
        if (cancelled) return;
        setUsers(loaded);
        setError(null);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setError(
          err instanceof ApiError && err.status === 403
            ? i18n.t('users.forbidden')
            : messageOf(err, i18n.t('users.loadFailed'))
        );
      })
      .finally(() => {
        if (!cancelled) setLoadedCount(reloadCount);
      });
    return () => {
      cancelled = true;
    };
  }, [reloadCount]);

  const replaceUser = (updated: UserAccount) => {
    setUsers((prev) => prev?.map((u) => (u.id === updated.id ? updated : u)) ?? null);
    setEditing((prev) => (prev?.id === updated.id ? updated : prev));
    if (updated.username === session.user_id) onSelfUpdated(updated);
  };

  const handleDelete = async (user: UserAccount) => {
    const label = user.display_name ? `${user.display_name} (${user.username})` : user.username;
    if (!window.confirm(t('users.confirmDelete', { label }))) return;
    try {
      await aegisApi.deleteUser(user.username);
      setUsers((prev) => prev?.filter((u) => u.id !== user.id) ?? null);
      onShowToast(t('users.deleted', { username: user.username }));
    } catch (err) {
      onShowToast(messageOf(err, t('api.deleteUserFailed')), 'error');
    }
  };

  const activeAdmins = users?.filter((u) => u.role === 'admin' && !u.disabled).length ?? 0;

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.heading}>
          <div className={styles.title}>
            <Users size={22} />
            <h1>{t('users.title')}</h1>
            {users && <span className={styles.count}>{users.length}</span>}
          </div>
          <div className={styles.headingActions}>
            <Button
              variant="tool"
              onClick={() => setReloadCount((n) => n + 1)}
              disabled={isLoading}
              title={t('common.reload')}
            >
              <RefreshCw size={14} />
              <span>{t('common.reload')}</span>
            </Button>
            <Button variant="secondary" onClick={() => setIsInviting(true)}>
              <Mail size={16} />
              <span>{t('users.inviteUser')}</span>
            </Button>
            <Button variant="primary" onClick={() => setIsCreating(true)}>
              <UserPlus size={16} />
              <span>{t('users.newUser')}</span>
            </Button>
          </div>
        </div>

        {error && <div className={cx(styles.notice, styles.error)}>{error}</div>}

        {users && (
          <div className={styles.tableWrap}>
            <table className={styles.table}>
              <thead>
                <tr>
                  <th>{t('users.columns.user')}</th>
                  <th>{t('users.columns.email')}</th>
                  <th>{t('users.columns.role')}</th>
                  <th>{t('users.columns.status')}</th>
                  <th>{t('users.twoFactor')}</th>
                  <th>{t('users.columns.created')}</th>
                  <th aria-label={t('users.columns.actions')} />
                </tr>
              </thead>
              <tbody>
                {users.map((user) => {
                  const isSelf = user.username === session.user_id;
                  return (
                    <tr key={user.id} className={cx(user.disabled && styles.disabledRow)}>
                      <td>
                        <div className={styles.userCell}>
                          <Avatar
                            url={user.avatar_url}
                            name={user.display_name || user.username}
                            size={32}
                          />
                          <div className={styles.userNames}>
                            <span className={styles.displayName}>
                              {user.display_name || user.username}
                              {isSelf && <span className={styles.you}>{t('users.you')}</span>}
                            </span>
                            <span className={styles.username}>@{user.username}</span>
                          </div>
                        </div>
                      </td>
                      <td className={cx(!user.email && styles.muted)}>{user.email || '—'}</td>
                      <td>
                        <span className={cx(styles.roleBadge, ROLE_BADGE[user.role])}>
                          <RoleLabel role={user.role} />
                        </span>
                      </td>
                      <td>
                        <span
                          className={cx(
                            styles.status,
                            user.disabled
                              ? user.pending_approval
                                ? styles.pending
                                : styles.off
                              : styles.on
                          )}
                        >
                          {user.disabled
                            ? user.pending_approval
                              ? t('users.pendingApproval')
                              : t('users.disabled')
                            : t('users.active')}
                        </span>
                      </td>
                      <td>
                        <span
                          className={cx(styles.status, user.totp_enabled ? styles.on : styles.off)}
                        >
                          {user.totp_enabled
                            ? t('users.twoFactorActive')
                            : t('users.twoFactorInactive')}
                        </span>
                      </td>
                      <td className={styles.muted}>{formatDate(user.created_at)}</td>
                      <td>
                        <div className={styles.rowActions}>
                          <Button
                            variant="icon"
                            onClick={() => setEditing(user)}
                            aria-label={t('users.editAria', { username: user.username })}
                            title={t('common.edit')}
                          >
                            <Pencil size={14} />
                          </Button>
                          <Button
                            variant="icon"
                            danger
                            onClick={() => handleDelete(user)}
                            disabled={isSelf}
                            aria-label={t('users.deleteAria', { username: user.username })}
                            title={isSelf ? t('users.cannotDeleteSelf') : t('common.delete')}
                          >
                            <Trash2 size={14} />
                          </Button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
                {users.length === 0 && (
                  <tr>
                    <td colSpan={6} className={styles.empty}>
                      {t('users.empty')}
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        )}

        <p className={styles.footnote}>{t('users.footnote')}</p>
      </div>

      {isCreating && (
        <CreateUserModal
          onCreated={(created) => {
            setUsers((prev) =>
              [...(prev ?? []), created].sort((a, b) => a.username.localeCompare(b.username))
            );
            setIsCreating(false);
            onShowToast(t('users.created', { username: created.username }));
          }}
          onClose={() => setIsCreating(false)}
        />
      )}

      {isInviting && (
        <InviteUserModal
          onInvited={(email) => {
            setIsInviting(false);
            onShowToast(t('users.invited', { email }));
          }}
          onClose={() => setIsInviting(false)}
        />
      )}

      {editing && (
        <EditUserModal
          user={editing}
          isSelf={editing.username === session.user_id}
          isLastAdmin={editing.role === 'admin' && !editing.disabled && activeAdmins <= 1}
          onUpdated={replaceUser}
          onShowToast={onShowToast}
          onClose={() => setEditing(null)}
        />
      )}
    </main>
  );
};

interface CreateUserModalProps {
  onCreated: (user: UserAccount) => void;
  onClose: () => void;
}

const CreateUserModal: React.FC<CreateUserModalProps> = ({ onCreated, onClose }) => {
  const { t } = useTranslation();
  const [username, setUsername] = useState<string>('');
  const [displayName, setDisplayName] = useState<string>('');
  const [email, setEmail] = useState<string>('');
  const [role, setRole] = useState<UserRole>('editor');
  const [password, setPassword] = useState<string>('');
  const [isSaving, setIsSaving] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const usernameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    usernameRef.current?.focus();
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!/^[A-Za-z0-9._@-]{1,64}$/.test(username.trim())) {
      setError(t('users.invalidUsername'));
      return;
    }
    if (password.length < MIN_PASSWORD_LENGTH) {
      setError(t('users.passwordTooShort', { count: MIN_PASSWORD_LENGTH }));
      return;
    }
    setIsSaving(true);
    setError(null);
    try {
      onCreated(
        await aegisApi.createUser({
          username: username.trim(),
          password,
          role,
          email: email.trim() || undefined,
          display_name: displayName.trim() || undefined,
        })
      );
    } catch (err) {
      setError(messageOf(err, t('users.createFailed')));
      setIsSaving(false);
    }
  };

  return (
    <Modal title={t('users.newUser')} icon={<UserPlus size={18} />} onClose={onClose} flush>
      <form className={formStyles.body} onSubmit={handleSubmit}>
        <div className={formStyles.section}>
          <FieldRow>
            <Field label={t('users.fields.username')}>
              <input
                ref={usernameRef}
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                placeholder={t('users.usernamePlaceholder')}
                autoComplete="off"
                maxLength={64}
                aria-label={t('users.aria.username')}
              />
            </Field>
            <Field label={t('users.fields.role')}>
              <select
                value={role}
                onChange={(e) => setRole(e.target.value as UserRole)}
                aria-label={t('users.aria.role')}
              >
                {ROLES.map((r) => (
                  <option key={r} value={r}>
                    {t(`common.roles.${r}`)}
                  </option>
                ))}
              </select>
            </Field>
          </FieldRow>
          <span className={formStyles.hint}>
            {t('users.usernameFixed', { description: t(`users.roleDescriptions.${role}`) })}
          </span>
          <Field label={t('users.fields.displayName')}>
            <input
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder={t('users.optional')}
              maxLength={100}
              aria-label={t('users.aria.displayName')}
            />
          </Field>
          <Field label={t('users.fields.email')}>
            <input
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder={t('users.optional')}
              aria-label={t('users.aria.email')}
            />
          </Field>
          <Field label={t('users.fields.password')}>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="new-password"
              placeholder={t('users.passwordPlaceholder', { count: MIN_PASSWORD_LENGTH })}
              aria-label={t('users.aria.initialPassword')}
            />
          </Field>
          {error && <span className={formStyles.error}>{error}</span>}
        </div>
        <div className={formStyles.actions}>
          <Button onClick={onClose}>{t('common.cancel')}</Button>
          <Button type="submit" variant="primary" disabled={isSaving}>
            <Plus size={16} />
            <span>{isSaving ? t('users.creating') : t('users.create')}</span>
          </Button>
        </div>
      </form>
    </Modal>
  );
};

interface EditUserModalProps {
  user: UserAccount;
  /** The signed-in admin: role and status are locked to avoid locking themselves out */
  isSelf: boolean;
  /** The only active admin cannot be demoted or disabled */
  isLastAdmin: boolean;
  onUpdated: (user: UserAccount) => void;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
  onClose: () => void;
}

const EditUserModal: React.FC<EditUserModalProps> = ({
  user,
  isSelf,
  isLastAdmin,
  onUpdated,
  onShowToast,
  onClose,
}) => {
  const { t } = useTranslation();
  const [displayName, setDisplayName] = useState<string>(user.display_name ?? '');
  const [email, setEmail] = useState<string>(user.email ?? '');
  const [role, setRole] = useState<UserRole>((user.role as UserRole) ?? 'viewer');
  const [disabled, setDisabled] = useState<boolean>(user.disabled);
  const [password, setPassword] = useState<string>('');
  const [isSaving, setIsSaving] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    nameRef.current?.focus();
  }, []);

  const lockAccess = isSelf || isLastAdmin;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (password && password.length < MIN_PASSWORD_LENGTH) {
      setError(t('users.newPasswordTooShort', { count: MIN_PASSWORD_LENGTH }));
      return;
    }
    if (password && isSelf) {
      setError(t('users.ownPassword'));
      return;
    }
    setIsSaving(true);
    setError(null);
    try {
      const updated = await aegisApi.updateUser(user.username, {
        display_name: displayName.trim(),
        email: email.trim(),
        ...(lockAccess ? {} : { role, disabled }),
        ...(password ? { password } : {}),
      });
      onUpdated(updated);
      onShowToast(t('users.saved', { username: user.username }));
      onClose();
    } catch (err) {
      setError(messageOf(err, t('users.saveFailed')));
      setIsSaving(false);
    }
  };

  return (
    <Modal
      title={t('users.editTitle', { username: user.username })}
      icon={<Pencil size={18} />}
      onClose={onClose}
      flush
    >
      <form className={formStyles.body} onSubmit={handleSubmit}>
        {user.pending_approval && disabled && (
          <div className={styles.pendingNotice}>
            <span>{t('users.pendingApprovalNotice')}</span>
            <Button type="button" variant="primary" onClick={() => setDisabled(false)}>
              <CheckCircle2 size={16} />
              <span>{t('users.approveAndActivate')}</span>
            </Button>
          </div>
        )}

        <div className={formStyles.section}>
          <AvatarPicker user={user} onChanged={onUpdated} />
        </div>

        <div className={formStyles.section}>
          <div className={formStyles.sectionTitle}>{t('users.profile')}</div>
          <Field label={t('users.fields.displayName')}>
            <input
              ref={nameRef}
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder={user.username}
              maxLength={100}
              aria-label={t('users.aria.displayName')}
            />
          </Field>
          <Field label={t('users.fields.email')}>
            <input
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder={t('users.optional')}
              aria-label={t('users.aria.email')}
            />
          </Field>
        </div>

        <div className={formStyles.section}>
          <div className={formStyles.sectionTitle}>{t('users.access')}</div>
          <Field label={t('users.fields.role')}>
            <select
              value={role}
              onChange={(e) => setRole(e.target.value as UserRole)}
              disabled={lockAccess}
              aria-label={t('users.aria.role')}
            >
              {ROLES.map((r) => (
                <option key={r} value={r}>
                  {t(`common.roles.${r}`)}
                </option>
              ))}
            </select>
          </Field>
          <span className={formStyles.hint}>{t(`users.roleDescriptions.${role}`)}</span>
          <label className={formStyles.checkbox}>
            <input
              type="checkbox"
              checked={disabled}
              onChange={(e) => setDisabled(e.target.checked)}
              disabled={lockAccess}
            />
            {t('users.disableAccount')}
          </label>
          {lockAccess && (
            <span className={formStyles.hint}>
              {isSelf ? t('users.lockedSelf') : t('users.lockedLastAdmin')}
            </span>
          )}
        </div>

        {user.totp_enabled && (
          <div className={formStyles.section}>
            <div className={formStyles.sectionTitle}>{t('users.twoFactor')}</div>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
              <span className={formStyles.hint}>{t('users.twoFactorActive')}</span>
              <Button
                type="button"
                variant="secondary"
                danger
                onClick={async () => {
                  if (window.confirm(t('users.reset2faConfirm', { username: user.username }))) {
                    try {
                      await aegisApi.resetUser2fa(user.username);
                      onUpdated({ ...user, totp_enabled: false });
                      onShowToast(t('users.reset2faSuccess', { username: user.username }));
                    } catch (err) {
                      setError(err instanceof Error ? err.message : 'Failed to reset 2FA');
                    }
                  }
                }}
              >
                {t('users.reset2fa')}
              </Button>
            </div>
          </div>
        )}

        {!isSelf && (
          <div className={formStyles.section}>
            <div className={formStyles.sectionTitle}>{t('users.resetPassword')}</div>
            <Field label={t('users.fields.newPassword')}>
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete="new-password"
                placeholder={t('users.keepPassword')}
                aria-label={t('users.aria.newPassword')}
              />
            </Field>
          </div>
        )}

        {error && <span className={formStyles.error}>{error}</span>}
        <div className={formStyles.actions}>
          <Button onClick={onClose}>{t('common.cancel')}</Button>
          <Button type="submit" variant="primary" disabled={isSaving}>
            <span>{isSaving ? t('users.saving') : t('users.save')}</span>
          </Button>
        </div>
      </form>
    </Modal>
  );
};

interface InviteUserModalProps {
  onInvited: (email: string) => void;
  onClose: () => void;
}

const InviteUserModal: React.FC<InviteUserModalProps> = ({ onInvited, onClose }) => {
  const { t } = useTranslation();
  const [email, setEmail] = useState<string>('');
  const [role, setRole] = useState<UserRole>('viewer');
  const [displayName, setDisplayName] = useState<string>('');
  const [isSending, setIsSending] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const emailRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    emailRef.current?.focus();
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!email.trim()) {
      setError(t('users.fields.email'));
      return;
    }
    setIsSending(true);
    setError(null);
    try {
      await aegisApi.createInvitation({
        email: email.trim(),
        role,
        ...(displayName.trim() ? { display_name: displayName.trim() } : {}),
      });
      onInvited(email.trim());
    } catch (err) {
      setError(messageOf(err, t('users.inviteFailed')));
      setIsSending(false);
    }
  };

  return (
    <Modal title={t('users.inviteTitle')} icon={<Mail size={18} />} onClose={onClose} flush>
      <form className={formStyles.body} onSubmit={handleSubmit}>
        <div className={formStyles.section}>
          <Field label={t('users.fields.email')}>
            <input
              ref={emailRef}
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="user@example.com"
              required
              aria-label={t('users.aria.email')}
            />
          </Field>
          <Field label={t('users.fields.role')}>
            <select
              value={role}
              onChange={(e) => setRole(e.target.value as UserRole)}
              aria-label={t('users.aria.role')}
            >
              {ROLES.map((r) => (
                <option key={r} value={r}>
                  {t(`common.roles.${r}`)}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t('users.fields.displayName')}>
            <input
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder={t('users.optional')}
              maxLength={100}
              aria-label={t('users.aria.displayName')}
            />
          </Field>
        </div>
        {error && <span className={formStyles.error}>{error}</span>}
        <div className={formStyles.actions}>
          <Button onClick={onClose}>{t('common.cancel')}</Button>
          <Button type="submit" variant="primary" disabled={isSending}>
            <Mail size={16} />
            <span>{isSending ? t('users.inviting') : t('users.invite')}</span>
          </Button>
        </div>
      </form>
    </Modal>
  );
};
