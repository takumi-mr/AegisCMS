import React, { useEffect, useRef, useState } from 'react';
import { Loader2, LogIn } from 'lucide-react';
import { aegisApi, ApiError } from '../api/client';
import type { SessionInfo } from '../types';
import { BrandMark } from './ui/BrandMark';
import { Button } from './ui/Button';
import styles from './LoginView.module.css';

interface LoginViewProps {
  onLoggedIn: (session: SessionInfo) => void;
  /** Shown above the form, e.g. after the previous session expired */
  notice?: string | null;
}

const describeError = (err: unknown): string => {
  if (err instanceof ApiError) {
    if (err.status === 401) return 'Invalid username or password.';
    return err.message;
  }
  // fetch rejects with a TypeError when the server cannot be reached
  return 'Cannot reach the AegisCMS server. Check that it is running.';
};

/** Full-screen username / password form shown until the user has a valid session */
export const LoginView: React.FC<LoginViewProps> = ({ onLoggedIn, notice }) => {
  const [username, setUsername] = useState<string>('');
  const [password, setPassword] = useState<string>('');
  const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const usernameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    usernameRef.current?.focus();
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!username.trim() || !password) {
      setError('Enter your username and password.');
      return;
    }
    setIsSubmitting(true);
    setError(null);
    try {
      onLoggedIn(await aegisApi.login(username.trim(), password));
    } catch (err) {
      setError(describeError(err));
      setPassword('');
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className={styles.screen}>
      <form className={styles.card} onSubmit={handleSubmit}>
        <div className={styles.brand}>
          <BrandMark />
          <div>
            <h1>AegisCMS</h1>
            <p>Sign in to Admin Studio</p>
          </div>
        </div>

        {notice && !error && <div className={styles.notice}>{notice}</div>}
        {error && (
          <div className={styles.error} role="alert">
            {error}
          </div>
        )}

        <label className={styles.field}>
          <span>Username</span>
          <input
            ref={usernameRef}
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            autoComplete="username"
            name="username"
          />
        </label>
        <label className={styles.field}>
          <span>Password</span>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="current-password"
            name="password"
          />
        </label>

        <Button type="submit" variant="primary" className={styles.submit} disabled={isSubmitting}>
          {isSubmitting ? <Loader2 size={16} className="animate-spin" /> : <LogIn size={16} />}
          <span>{isSubmitting ? 'Signing in...' : 'Sign in'}</span>
        </Button>

        <p className={styles.hint}>
          No account yet? Create one on the server with{' '}
          <code>aegis-server user add &lt;username&gt; --role admin</code>.
        </p>
      </form>
    </div>
  );
};

/** Placeholder shown while the stored session token is being checked */
export const SessionCheckingScreen: React.FC = () => (
  <div className={styles.screen}>
    <span className={styles.checking}>Checking session...</span>
  </div>
);
