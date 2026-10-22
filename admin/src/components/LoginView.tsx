import React, { useEffect, useRef, useState } from 'react';
import { Loader2, LogIn, ShieldCheck } from 'lucide-react';
import { aegisApi, ApiError } from '../api/client';
import type { SessionInfo } from '../types';

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
    <div className="login-screen">
      <form className="login-card" onSubmit={handleSubmit}>
        <div className="login-brand">
          <div className="brand-icon">
            <ShieldCheck size={20} color="#fff" />
          </div>
          <div>
            <h1>AegisCMS</h1>
            <p>Sign in to Admin Studio</p>
          </div>
        </div>

        {notice && !error && <div className="login-notice">{notice}</div>}
        {error && (
          <div className="login-error" role="alert">
            {error}
          </div>
        )}

        <label className="login-field">
          <span>Username</span>
          <input
            ref={usernameRef}
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            autoComplete="username"
            name="username"
          />
        </label>
        <label className="login-field">
          <span>Password</span>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="current-password"
            name="password"
          />
        </label>

        <button type="submit" className="btn btn-primary login-submit" disabled={isSubmitting}>
          {isSubmitting ? <Loader2 size={16} className="animate-spin" /> : <LogIn size={16} />}
          <span>{isSubmitting ? 'Signing in...' : 'Sign in'}</span>
        </button>

        <p className="login-hint">
          No account yet? Create one on the server with{' '}
          <code>aegis-server user add &lt;username&gt; --role admin</code>.
        </p>
      </form>
    </div>
  );
};
