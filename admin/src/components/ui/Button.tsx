import React from 'react';
import { cx } from '../../utils/cx';
import styles from './Button.module.css';

export type ButtonVariant = 'primary' | 'secondary' | 'tool' | 'icon';

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  /** primary / secondary: regular buttons, tool: compact toolbar button, icon: icon only */
  variant?: ButtonVariant;
  /** Red hover for destructive icon buttons */
  danger?: boolean;
}

/** Button with the design-system styles. Defaults to `type="button"` so it never submits a form by accident */
export const Button: React.FC<ButtonProps> = ({
  variant = 'secondary',
  danger = false,
  type = 'button',
  className,
  ...rest
}) => (
  <button
    type={type}
    className={cx(styles[variant], danger && styles.danger, className)}
    {...rest}
  />
);
