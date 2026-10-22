import React from 'react';
import { ShieldCheck } from 'lucide-react';
import styles from './BrandMark.module.css';

/** AegisCMS logo mark (gradient tile with a shield) */
export const BrandMark: React.FC = () => (
  <div className={styles.mark}>
    <ShieldCheck size={20} color="#fff" />
  </div>
);
