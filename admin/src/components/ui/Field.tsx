import React from 'react';
import { cx } from '../../utils/cx';
import styles from './Field.module.css';

interface FieldProps {
  /** Short inline label shown before the control, e.g. `slug:` */
  label?: React.ReactNode;
  /** Render as `<label>` so clicking anywhere focuses the control */
  as?: 'div' | 'label';
  className?: string;
  children: React.ReactNode;
}

/** Bordered container for inputs / selects with an optional inline label */
export const Field: React.FC<FieldProps> = ({ label, as: Tag = 'div', className, children }) => (
  <Tag className={cx(styles.field, className)}>
    {label && <span className={styles.label}>{label}</span>}
    {children}
  </Tag>
);

/** Horizontal row of fields */
export const FieldRow: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div className={styles.row}>{children}</div>
);
