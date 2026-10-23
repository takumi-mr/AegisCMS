import React, { useRef, useState } from 'react';
import { ImagePlus, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type { UserAccount } from '../types';
import { Avatar } from './ui/Avatar';
import { Button } from './ui/Button';
import styles from './UserForms.module.css';

// Mirrors the server's checks (`PUT /api/v1/users/{username}/avatar`)
const ACCEPTED_TYPES = ['image/png', 'image/jpeg', 'image/gif', 'image/webp'];
const MAX_AVATAR_BYTES = 2 * 1024 * 1024;

interface AvatarPickerProps {
  user: UserAccount;
  /** Called with the account returned by the server after an upload or removal */
  onChanged: (user: UserAccount) => void;
}

/** Large avatar with buttons to upload a new image or remove the current one */
export const AvatarPicker: React.FC<AvatarPickerProps> = ({ user, onChanged }) => {
  const { t } = useTranslation();
  const inputRef = useRef<HTMLInputElement>(null);
  const [isBusy, setIsBusy] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (action: () => Promise<UserAccount>) => {
    setIsBusy(true);
    setError(null);
    try {
      onChanged(await action());
    } catch (err) {
      setError(err instanceof Error ? err.message : t('avatar.failed'));
    } finally {
      setIsBusy(false);
    }
  };

  const handleFile = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    // Allow choosing the same file again after an error
    e.target.value = '';
    if (!file) return;
    if (!ACCEPTED_TYPES.includes(file.type)) {
      setError(t('avatar.wrongType'));
      return;
    }
    if (file.size > MAX_AVATAR_BYTES) {
      setError(t('avatar.tooLarge'));
      return;
    }
    run(() => aegisApi.uploadAvatar(user.username, file));
  };

  return (
    <div className={styles.avatarPicker}>
      <Avatar url={user.avatar_url} name={user.display_name || user.username} size={72} />
      <div className={styles.avatarActions}>
        <div className={styles.avatarButtons}>
          <Button variant="tool" onClick={() => inputRef.current?.click()} disabled={isBusy}>
            <ImagePlus size={14} />
            <span>
              {isBusy
                ? t('avatar.uploading')
                : user.avatar_url
                  ? t('avatar.change')
                  : t('avatar.upload')}
            </span>
          </Button>
          {user.avatar_url && (
            <Button
              variant="tool"
              onClick={() => run(() => aegisApi.deleteAvatar(user.username))}
              disabled={isBusy}
            >
              <Trash2 size={14} />
              <span>{t('avatar.remove')}</span>
            </Button>
          )}
        </div>
        <span className={styles.hint}>{t('avatar.hint')}</span>
        {error && <span className={styles.error}>{error}</span>}
      </div>
      <input
        ref={inputRef}
        type="file"
        accept={ACCEPTED_TYPES.join(',')}
        onChange={handleFile}
        hidden
        aria-label={t('avatar.aria')}
      />
    </div>
  );
};
