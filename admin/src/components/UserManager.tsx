import React, { useEffect, useRef, useState } from 'react';
import { Pencil, Plus, RefreshCw, Trash2, UserPlus, Users } from 'lucide-react';
import { aegisApi, ApiError } from '../api/client';
import type { SessionInfo, UserAccount, UserRole } from '../types';
import { cx } from '../utils/cx';
import { AvatarPicker } from './AvatarPicker';
import { MIN_PASSWORD_LENGTH } from './ProfileModal';
import { Avatar } from './ui/Avatar';
import { Button } from './ui/Button';
import { Field, FieldRow } from './ui/Field';
import { Modal } from './ui/Modal';
import styles from './UserManager.module.css';
import formStyles from './UserForms.module.css';

const ROLES: UserRole[] = ['admin', 'editor', 'viewer'];

const ROLE_BADGE: Record<string, string | undefined> = {
  admin: styles.roleAdmin,
  editor: styles.roleEditor,
};

const ROLE_DESCRIPTIONS: Record<UserRole, string> = {
  admin: 'Everything, including users, themes and audit logs',
  editor: 'Create content and categories, upload media',
  viewer: 'Read content and media',
};

const formatDate = (iso: string): string => {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleDateString();
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
  const [users, setUsers] = useState<UserAccount[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Loading = the last finished request is older than the latest reload
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [loadedCount, setLoadedCount] = useState<number>(-1);
  const isLoading = loadedCount !== reloadCount;
  const [editing, setEditing] = useState<UserAccount | null>(null);
  const [isCreating, setIsCreating] = useState<boolean>(false);

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
            ? 'Managing users requires the "admin" role.'
            : messageOf(err, 'Failed to load users')
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
    if (!window.confirm(`Delete the user ${label}? This cannot be undone.`)) return;
    try {
      await aegisApi.deleteUser(user.username);
      setUsers((prev) => prev?.filter((u) => u.id !== user.id) ?? null);
      onShowToast(`Deleted user "${user.username}"`);
    } catch (err) {
      onShowToast(messageOf(err, 'Failed to delete the user'), 'error');
    }
  };

  const activeAdmins = users?.filter((u) => u.role === 'admin' && !u.disabled).length ?? 0;

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.heading}>
          <div className={styles.title}>
            <Users size={22} />
            <h1>Users</h1>
            {users && <span className={styles.count}>{users.length}</span>}
          </div>
          <div className={styles.headingActions}>
            <Button
              variant="tool"
              onClick={() => setReloadCount((n) => n + 1)}
              disabled={isLoading}
              title="Reload"
            >
              <RefreshCw size={14} />
              <span>Reload</span>
            </Button>
            <Button variant="primary" onClick={() => setIsCreating(true)}>
              <UserPlus size={16} />
              <span>New user</span>
            </Button>
          </div>
        </div>

        {error && <div className={cx(styles.notice, styles.error)}>{error}</div>}

        {users && (
          <div className={styles.tableWrap}>
            <table className={styles.table}>
              <thead>
                <tr>
                  <th>User</th>
                  <th>Email</th>
                  <th>Role</th>
                  <th>Status</th>
                  <th>Created</th>
                  <th aria-label="Actions" />
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
                              {isSelf && <span className={styles.you}>you</span>}
                            </span>
                            <span className={styles.username}>@{user.username}</span>
                          </div>
                        </div>
                      </td>
                      <td className={cx(!user.email && styles.muted)}>{user.email || '—'}</td>
                      <td>
                        <span className={cx(styles.roleBadge, ROLE_BADGE[user.role])}>
                          {user.role}
                        </span>
                      </td>
                      <td>
                        <span className={cx(styles.status, user.disabled ? styles.off : styles.on)}>
                          {user.disabled ? 'Disabled' : 'Active'}
                        </span>
                      </td>
                      <td className={styles.muted}>{formatDate(user.created_at)}</td>
                      <td>
                        <div className={styles.rowActions}>
                          <Button
                            variant="icon"
                            onClick={() => setEditing(user)}
                            aria-label={`Edit ${user.username}`}
                            title="Edit"
                          >
                            <Pencil size={14} />
                          </Button>
                          <Button
                            variant="icon"
                            danger
                            onClick={() => handleDelete(user)}
                            disabled={isSelf}
                            aria-label={`Delete ${user.username}`}
                            title={isSelf ? 'You cannot delete your own account' : 'Delete'}
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
                      No users yet.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        )}

        <p className={styles.footnote}>
          Role changes and disabling apply from the user's next sign-in; sessions that are already
          open stay valid until they expire.
        </p>
      </div>

      {isCreating && (
        <CreateUserModal
          onCreated={(created) => {
            setUsers((prev) =>
              [...(prev ?? []), created].sort((a, b) => a.username.localeCompare(b.username))
            );
            setIsCreating(false);
            onShowToast(`Created user "${created.username}"`);
          }}
          onClose={() => setIsCreating(false)}
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
      setError("Username must be 1-64 characters of letters, digits, '.', '_', '-' or '@'.");
      return;
    }
    if (password.length < MIN_PASSWORD_LENGTH) {
      setError(`The password must be at least ${MIN_PASSWORD_LENGTH} characters.`);
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
      setError(messageOf(err, 'Failed to create the user'));
      setIsSaving(false);
    }
  };

  return (
    <Modal title="New user" icon={<UserPlus size={18} />} onClose={onClose}>
      <form className={formStyles.body} onSubmit={handleSubmit}>
        <div className={formStyles.section}>
          <FieldRow>
            <Field label="username:">
              <input
                ref={usernameRef}
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                placeholder="editor1"
                autoComplete="off"
                maxLength={64}
                aria-label="Username"
              />
            </Field>
            <Field label="role:">
              <select
                value={role}
                onChange={(e) => setRole(e.target.value as UserRole)}
                aria-label="Role"
              >
                {ROLES.map((r) => (
                  <option key={r} value={r}>
                    {r}
                  </option>
                ))}
              </select>
            </Field>
          </FieldRow>
          <span className={formStyles.hint}>
            {ROLE_DESCRIPTIONS[role]}. The username cannot be changed later.
          </span>
          <Field label="display name:">
            <input
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder="Optional"
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
          <Field label="password:">
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="new-password"
              placeholder={`At least ${MIN_PASSWORD_LENGTH} characters`}
              aria-label="Initial password"
            />
          </Field>
          {error && <span className={formStyles.error}>{error}</span>}
        </div>
        <div className={formStyles.actions}>
          <Button onClick={onClose}>Cancel</Button>
          <Button type="submit" variant="primary" disabled={isSaving}>
            <Plus size={16} />
            <span>{isSaving ? 'Creating...' : 'Create user'}</span>
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
      setError(`The new password must be at least ${MIN_PASSWORD_LENGTH} characters.`);
      return;
    }
    if (password && isSelf) {
      setError('Change your own password from "My profile" (it asks for the current one).');
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
      onShowToast(`Saved user "${user.username}"`);
      onClose();
    } catch (err) {
      setError(messageOf(err, 'Failed to save the user'));
      setIsSaving(false);
    }
  };

  return (
    <Modal title={`Edit ${user.username}`} icon={<Pencil size={18} />} onClose={onClose}>
      <form className={formStyles.body} onSubmit={handleSubmit}>
        <div className={formStyles.section}>
          <AvatarPicker user={user} onChanged={onUpdated} />
        </div>

        <div className={formStyles.section}>
          <div className={formStyles.sectionTitle}>Profile</div>
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
        </div>

        <div className={formStyles.section}>
          <div className={formStyles.sectionTitle}>Access</div>
          <Field label="role:">
            <select
              value={role}
              onChange={(e) => setRole(e.target.value as UserRole)}
              disabled={lockAccess}
              aria-label="Role"
            >
              {ROLES.map((r) => (
                <option key={r} value={r}>
                  {r}
                </option>
              ))}
            </select>
          </Field>
          <span className={formStyles.hint}>{ROLE_DESCRIPTIONS[role] ?? ''}</span>
          <label className={formStyles.checkbox}>
            <input
              type="checkbox"
              checked={disabled}
              onChange={(e) => setDisabled(e.target.checked)}
              disabled={lockAccess}
            />
            Disable this account (it can no longer sign in)
          </label>
          {lockAccess && (
            <span className={formStyles.hint}>
              {isSelf
                ? 'You cannot change your own role or disable your own account.'
                : 'This is the only active admin: promote another user first.'}
            </span>
          )}
        </div>

        {!isSelf && (
          <div className={formStyles.section}>
            <div className={formStyles.sectionTitle}>Reset password</div>
            <Field label="new password:">
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete="new-password"
                placeholder="Leave empty to keep the current password"
                aria-label="New password"
              />
            </Field>
          </div>
        )}

        {error && <span className={formStyles.error}>{error}</span>}
        <div className={formStyles.actions}>
          <Button onClick={onClose}>Cancel</Button>
          <Button type="submit" variant="primary" disabled={isSaving}>
            <span>{isSaving ? 'Saving...' : 'Save changes'}</span>
          </Button>
        </div>
      </form>
    </Modal>
  );
};
