import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { aegisApi, ApiError } from '../api/client';
import type { StoreUpdatesResponse } from '../types';
import { NO_STORE_UPDATES, StoreUpdatesContext, type StoreUpdates } from '../storeUpdates';

/** Provides the update check to Admin Studio. Only administrators use the store API */
export const StoreUpdatesProvider: React.FC<{ enabled: boolean; children: React.ReactNode }> = ({
  enabled,
  children,
}) => {
  const [status, setStatus] = useState<StoreUpdates['status']>('idle');
  const [result, setResult] = useState<StoreUpdatesResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  const apply = useCallback((answer: StoreUpdatesResponse) => {
    setResult(answer);
    setError(null);
    setStatus('ready');
  }, []);
  const fail = useCallback((err: unknown) => {
    if (err instanceof ApiError && err.code === 'store_disabled') {
      setStatus('disabled');
    } else {
      setError(err instanceof Error ? err.message : String(err));
      setStatus('error');
    }
  }, []);

  const refresh = useCallback(
    async (force = false) => {
      if (!enabled) return;
      setStatus('loading');
      try {
        apply(await aegisApi.getStoreUpdates(force));
      } catch (err) {
        fail(err);
      }
    },
    [enabled, apply, fail]
  );

  useEffect(() => {
    // Once per opening of Admin Studio (the server decides whether the catalogs are due)
    if (!enabled) return;
    let cancelled = false;
    aegisApi
      .getStoreUpdates(false)
      .then((answer) => {
        if (!cancelled) apply(answer);
      })
      .catch((err: unknown) => {
        if (!cancelled) fail(err);
      });
    return () => {
      cancelled = true;
    };
  }, [enabled, apply, fail]);

  const value = useMemo<StoreUpdates>(
    () => (enabled ? { status, result, error, refresh } : NO_STORE_UPDATES),
    [enabled, status, result, error, refresh]
  );
  return <StoreUpdatesContext.Provider value={value}>{children}</StoreUpdatesContext.Provider>;
};
