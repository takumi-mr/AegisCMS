import React, { useRef, useState } from 'react';
import { Loader2, UploadCloud } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type { StorageMeta } from '../types';
import { cx } from '../utils/cx';
import { formatSize } from '../utils/media';
import styles from './ImageUploader.module.css';

interface ImageUploaderProps {
  /** Called with the stored asset and the original file once the upload succeeds */
  onUploaded: (meta: StorageMeta, file: File) => void;
}

/**
 * Drop zone for image blocks: accepts a dropped or chosen image file and uploads it
 * to `/api/v1/storage/upload`. The server enforces the size limit (`storage.max_upload_mb`).
 */
export const ImageUploader: React.FC<ImageUploaderProps> = ({ onUploaded }) => {
  const { t } = useTranslation();
  const inputRef = useRef<HTMLInputElement>(null);
  const [isDragging, setIsDragging] = useState<boolean>(false);
  const [uploading, setUploading] = useState<File | null>(null);
  const [error, setError] = useState<string | null>(null);

  const upload = async (file: File | undefined) => {
    if (!file || uploading) return;
    if (!file.type.startsWith('image/')) {
      setError(t('upload.notImage', { name: file.name }));
      return;
    }
    setError(null);
    setUploading(file);
    try {
      onUploaded(await aegisApi.uploadFile(file), file);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('upload.failed'));
    } finally {
      setUploading(null);
    }
  };

  return (
    <div>
      <div
        className={cx(
          styles.dropzone,
          isDragging && styles.dragging,
          uploading && styles.uploading
        )}
        role="button"
        tabIndex={0}
        aria-label={t('upload.aria')}
        onClick={() => inputRef.current?.click()}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            inputRef.current?.click();
          }
        }}
        onDragOver={(e) => {
          e.preventDefault();
          setIsDragging(true);
        }}
        onDragLeave={() => setIsDragging(false)}
        onDrop={(e) => {
          e.preventDefault();
          setIsDragging(false);
          upload(e.dataTransfer.files[0]);
        }}
      >
        {uploading ? (
          <>
            <Loader2 size={18} className="animate-spin" />
            <span>
              {t('upload.uploading', { name: uploading.name, size: formatSize(uploading.size) })}
            </span>
          </>
        ) : (
          <>
            <UploadCloud size={18} />
            <span>{t('upload.drop')}</span>
          </>
        )}
      </div>
      <input
        ref={inputRef}
        type="file"
        accept="image/*"
        hidden
        data-testid="image-file-input"
        onChange={(e) => {
          upload(e.target.files?.[0]);
          // Allow choosing the same file again after an error
          e.target.value = '';
        }}
      />
      {error && <div className={styles.error}>{error}</div>}
    </div>
  );
};
