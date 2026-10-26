import React, { useEffect, useId, useRef, useState } from 'react';
import { X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cx } from '../../utils/cx';
import { Button } from './Button';
import styles from './Modal.module.css';

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

interface ModalProps {
  title: string;
  icon: React.ReactNode;
  onClose: () => void;
  /** Extra header buttons shown before the close button */
  actions?: React.ReactNode;
  /** 860px wide instead of 640px */
  wide?: boolean;
  /** Disable the standard content inset when children already provide their own padding */
  flush?: boolean;
  children: React.ReactNode;
}

/**
 * Dialog with a dimmed overlay. Closes on Escape or a click outside, keeps Tab focus inside
 * the dialog and returns focus to the element that opened it. Children focus their own
 * first field on mount.
 */
export const Modal: React.FC<ModalProps> = ({
  title,
  icon,
  onClose,
  actions,
  wide,
  flush = false,
  children,
}) => {
  const { t } = useTranslation();
  const titleId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  // Captured during the first render, before children move focus into the dialog
  const [opener] = useState(() => document.activeElement as HTMLElement | null);

  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCloseRef.current();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      opener?.focus();
    };
  }, [opener]);

  const trapTab = (e: React.KeyboardEvent) => {
    if (e.key !== 'Tab' || !dialogRef.current) return;
    const focusable = [...dialogRef.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  };

  const closeButton = (
    <Button variant="icon" onClick={onClose} aria-label={t('common.close')}>
      <X size={16} />
    </Button>
  );

  return (
    <div
      className={styles.overlay}
      onMouseDown={(e) => {
        // Without preventDefault the browser moves focus to <body> after we restore it
        e.preventDefault();
        onClose();
      }}
    >
      <div
        ref={dialogRef}
        className={cx(styles.modal, wide && styles.wide)}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={trapTab}
      >
        <div className={styles.header}>
          <div className={styles.title}>
            {icon}
            <h2 id={titleId}>{title}</h2>
          </div>
          {actions ? (
            <div className={styles.actions}>
              {actions}
              {closeButton}
            </div>
          ) : (
            closeButton
          )}
        </div>
        <div className={cx(styles.body, flush && styles.flush)}>{children}</div>
      </div>
    </div>
  );
};
