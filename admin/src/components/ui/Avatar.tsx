import React, { useState } from 'react';
import { serverAssetUrl } from '../../api/client';
import { cx } from '../../utils/cx';
import styles from './Avatar.module.css';

interface AvatarProps {
  /** Avatar image URL from the API (`avatar_url`), or null for the initials */
  url: string | null | undefined;
  /** Display name or username, used for the initials and the fallback color */
  name: string;
  /** Diameter in pixels */
  size?: number;
  className?: string;
}

const PALETTE = ['#6366f1', '#8b5cf6', '#ec4899', '#f43f5e', '#f59e0b', '#10b981', '#06b6d4'];

const initialsOf = (name: string): string => {
  const words = name
    .trim()
    .split(/[\s._@-]+/)
    .filter(Boolean);
  if (words.length === 0) return '?';
  const letters = words.length > 1 ? words[0][0] + words[1][0] : words[0].slice(0, 2);
  return letters.toUpperCase();
};

const colorOf = (name: string): string => {
  let hash = 0;
  for (const ch of name) hash = (hash * 31 + ch.charCodeAt(0)) | 0;
  return PALETTE[Math.abs(hash) % PALETTE.length];
};

/** Round user picture; shows the initials when there is no image or it fails to load */
export const Avatar: React.FC<AvatarProps> = ({ url, name, size = 28, className }) => {
  // Remember which URL failed so a new upload is tried again
  const [failedUrl, setFailedUrl] = useState<string | null>(null);
  const style = { width: size, height: size, fontSize: Math.max(10, size * 0.38) };

  if (url && url !== failedUrl) {
    return (
      <img
        className={cx(styles.avatar, className)}
        style={style}
        src={serverAssetUrl(url)}
        alt=""
        onError={() => setFailedUrl(url)}
      />
    );
  }
  return (
    <span
      className={cx(styles.avatar, styles.initials, className)}
      style={{ ...style, background: colorOf(name) }}
      aria-hidden="true"
    >
      {initialsOf(name)}
    </span>
  );
};
