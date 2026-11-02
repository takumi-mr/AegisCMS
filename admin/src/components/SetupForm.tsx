import React, { useState } from 'react';
import { Loader2, ShieldCheck } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, ApiError } from '../api/client';
import type { SessionInfo } from '../types';
import { Button } from './ui/Button';
import styles from './LoginView.module.css';

interface SetupFormProps {
  onSetupComplete: (session: SessionInfo) => void;
  onBack?: () => void;
}

export const SetupForm: React.FC<SetupFormProps> = ({ onSetupComplete, onBack }) => {
  const { t } = useTranslation();
  const [siteTitle, setSiteTitle] = useState('AegisCMS');
  const [adminUsername, setAdminUsername] = useState('admin');
  const [adminPassword, setAdminPassword] = useState('');
  const [adminEmail, setAdminEmail] = useState('');
  const [createSampleData, setCreateSampleData] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!adminUsername.trim() || !adminPassword) {
      setError(t('login.errors.missing'));
      return;
    }
    if (adminPassword.length < 8) {
      setError(
        t('login.setup.errors.passwordLength', 'Password must be at least 8 characters long')
      );
      return;
    }
    setIsSubmitting(true);
    setError(null);
    try {
      await aegisApi.runSetup({
        site_title: siteTitle.trim() || 'AegisCMS',
        admin_username: adminUsername.trim(),
        admin_password: adminPassword,
        admin_email: adminEmail.trim() || null,
        create_sample_data: createSampleData,
      });

      // Automatically log in with the newly created administrator account
      const loginRes = await aegisApi.login(adminUsername.trim(), adminPassword);
      if (!('mfa_required' in loginRes)) {
        onSetupComplete(loginRes);
      } else if (onBack) {
        onBack();
      }
    } catch (err) {
      if (err instanceof ApiError) {
        setError(err.message);
      } else {
        setError(t('common.unreachable'));
      }
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <form className={styles.form} onSubmit={handleSubmit}>
      <div style={{ textAlign: 'center', marginBottom: '8px' }}>
        <h2 style={{ fontSize: '1.15rem', fontWeight: 600 }}>
          {t('login.setup.title', 'Initial Setup')}
        </h2>
        <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: '4px' }}>
          {t('login.setup.subtitle', 'Configure your site and create the first administrator.')}
        </p>
      </div>

      {error && <div className={styles.error}>{error}</div>}

      <div className={styles.field}>
        <label htmlFor="setup-site-title">{t('login.setup.siteTitle', 'Site Title')}</label>
        <input
          id="setup-site-title"
          type="text"
          value={siteTitle}
          onChange={(e) => setSiteTitle(e.target.value)}
          required
        />
      </div>

      <div className={styles.field}>
        <label htmlFor="setup-admin-username">
          {t('login.setup.adminUsername', 'Admin Username')}
        </label>
        <input
          id="setup-admin-username"
          type="text"
          value={adminUsername}
          onChange={(e) => setAdminUsername(e.target.value)}
          required
        />
      </div>

      <div className={styles.field}>
        <label htmlFor="setup-admin-password">
          {t('login.setup.adminPassword', 'Admin Password (min. 8 chars)')}
        </label>
        <input
          id="setup-admin-password"
          type="password"
          value={adminPassword}
          onChange={(e) => setAdminPassword(e.target.value)}
          required
          minLength={8}
        />
      </div>

      <div className={styles.field}>
        <label htmlFor="setup-admin-email">
          {t('login.setup.adminEmail', 'Admin Email (optional)')}
        </label>
        <input
          id="setup-admin-email"
          type="email"
          value={adminEmail}
          onChange={(e) => setAdminEmail(e.target.value)}
          placeholder="admin@example.com"
        />
      </div>

      <div style={{ display: 'flex', alignItems: 'center', gap: '8px', margin: '4px 0 12px 0' }}>
        <input
          id="setup-sample-data"
          type="checkbox"
          checked={createSampleData}
          onChange={(e) => setCreateSampleData(e.target.checked)}
          style={{ accentColor: 'var(--accent-primary)', width: '16px', height: '16px' }}
        />
        <label
          htmlFor="setup-sample-data"
          style={{ fontSize: '0.85rem', cursor: 'pointer', margin: 0 }}
        >
          {t('login.setup.sampleData', 'Create sample content (welcome article & page)')}
        </label>
      </div>

      <div className={styles.submit}>
        <Button type="submit" variant="primary" disabled={isSubmitting}>
          {isSubmitting ? (
            <>
              <Loader2 size={16} className={styles.spinner} />
              <span>{t('login.setup.submitting', 'Setting up...')}</span>
            </>
          ) : (
            <>
              <ShieldCheck size={16} />
              <span>{t('login.setup.submit', 'Complete Installation')}</span>
            </>
          )}
        </Button>
      </div>

      {onBack && (
        <div className={styles.links}>
          <button type="button" className={styles.linkButton} onClick={onBack}>
            {t('login.backToLogin', 'Back to sign-in')}
          </button>
        </div>
      )}
    </form>
  );
};
