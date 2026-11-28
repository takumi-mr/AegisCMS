import { createContext, useContext } from 'react';
import type {
  StorePackageKind,
  StoreUpdate,
  StoreUpdateCheckStatus,
  StoreUpdatesResponse,
} from './types';

/** The update check (docs/store.md 8): checked once when Admin Studio opens, and on "Check now" */
export interface StoreUpdates {
  status: 'idle' | 'loading' | 'ready' | 'disabled' | 'error';
  /** The last response (`null` until one arrived) */
  result: StoreUpdatesResponse | null;
  /** Why the request itself failed (`status === 'error'`) */
  error: string | null;
  /** Asks the server again. `force` fetches the catalogs now ("Check now"); otherwise the server
   *  uses its cache and fetches only when the check is due */
  refresh: (force?: boolean) => Promise<void>;
}

/** For non-administrators and screens outside the provider: nothing to show */
export const NO_STORE_UPDATES: StoreUpdates = {
  status: 'idle',
  result: null,
  error: null,
  refresh: async () => {},
};

export const StoreUpdatesContext = createContext<StoreUpdates>(NO_STORE_UPDATES);

export const useStoreUpdates = (): StoreUpdates => useContext(StoreUpdatesContext);

/** The updates of one kind */
export const updatesOf = (state: StoreUpdates, kind: StorePackageKind): StoreUpdate[] =>
  state.result?.updates.filter((u) => u.kind === kind) ?? [];

/** The newest check time over the catalogs that could be compared */
export const lastChecked = (catalogs: StoreUpdateCheckStatus[]): string | undefined =>
  catalogs
    .map((c) => c.checked_at)
    .filter((at): at is string => !!at)
    .sort()
    .at(-1);
