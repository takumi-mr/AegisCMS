import React, { useCallback, useEffect, useState } from 'react';
import {
  AlertTriangle,
  BadgeCheck,
  Download,
  PackageSearch,
  RefreshCw,
  Search,
  Star,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { aegisApi, ApiError } from '../api/client';
import i18n from '../i18n';
import { errorText } from '../utils/storeErrors';
import type { StoreCatalogStatus, StorePackage, StorePackageKind, StoreVersion } from '../types';
import {
  capabilityAdditions,
  storeCapabilities,
  type StoreCapability,
} from '../utils/storeCapabilities';
import { useStoreUpdates } from '../storeUpdates';
import { Button } from './ui/Button';
import { Modal } from './ui/Modal';
import styles from './StorePanel.module.css';

interface StorePanelProps {
  /** Which packages to list */
  kind: StorePackageKind;
  /** The store API needs the admin role */
  canInstall: boolean;
  /** Called after a package was installed (the parent reloads its installed list) */
  onInstalled: () => void;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

const errorOf = (err: unknown, fallback: string): string =>
  err instanceof ApiError
    ? errorText(err.code, err.message, fallback)
    : err instanceof Error
      ? err.message
      : fallback;

const capabilityText = (item: StoreCapability, t: TFunction): string => {
  switch (item.kind) {
    case 'hooks':
      return t('store.cap.hooks', { hooks: item.values.join(', ') });
    case 'storage':
      return t('store.cap.storage');
    case 'network':
      return t('store.cap.network', { hosts: item.values.join(', ') });
    case 'frames':
      return t('store.cap.frames', { hosts: item.values.join(', ') });
    case 'siteFrames':
      return t('store.cap.siteFrames');
    case 'scripts':
      return t('store.cap.scripts', {
        kinds: [item.inline && t('plugins.inlineScripts'), item.local && t('plugins.localScripts')]
          .filter(Boolean)
          .join(', '),
      });
    case 'csp':
      return t('store.cap.csp', { directive: item.directive, hosts: item.values.join(', ') });
  }
};

const formatSize = (bytes: number): string =>
  bytes >= 1024 * 1024 ? `${(bytes / 1024 / 1024).toFixed(1)} MB` : `${Math.ceil(bytes / 1024)} KB`;

const keyOf = (pkg: StorePackage) => `${pkg.catalog}/${pkg.id}`;

type LoadState =
  | { status: 'loading' }
  | { status: 'ready'; catalogs: StoreCatalogStatus[]; packages: StorePackage[] }
  | { status: 'disabled' }
  | { status: 'error'; message: string };

/**
 * Packages of one kind from the configured store catalogs ([store], docs/store.md), with
 * install and update. Installing asks first, showing what the package will be allowed to do.
 */
export const StorePanel: React.FC<StorePanelProps> = ({
  kind,
  canInstall,
  onInstalled,
  onShowToast,
}) => {
  const { t } = useTranslation();
  const storeUpdates = useStoreUpdates();
  const [state, setState] = useState<LoadState>({ status: 'loading' });
  const [reload, setReload] = useState(0);
  const [query, setQuery] = useState('');
  const [confirm, setConfirm] = useState<{ pkg: StorePackage; version: StoreVersion } | null>(null);
  const [installing, setInstalling] = useState<string | null>(null);

  useEffect(() => {
    if (!canInstall) return;
    let cancelled = false;
    aegisApi
      .listStorePackages()
      .then((data) => {
        if (!cancelled) setState({ status: 'ready', ...data });
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        if (err instanceof ApiError && err.code === 'store_disabled') {
          setState({ status: 'disabled' });
        } else {
          setState({ status: 'error', message: errorOf(err, i18n.t('api.storeFailed')) });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [canInstall, reload]);

  const refresh = useCallback(() => {
    setState({ status: 'loading' });
    setReload((n) => n + 1);
  }, []);

  const install = async () => {
    if (!confirm) return;
    const { pkg, version } = confirm;
    setInstalling(keyOf(pkg));
    try {
      const done = await aegisApi.installStorePackage({
        catalog: pkg.catalog,
        kind: pkg.kind,
        id: pkg.id,
        version: version.version,
      });
      onShowToast(t('store.installSuccess', { name: pkg.name, version: done.version }), 'success');
      setConfirm(null);
      refresh();
      onInstalled();
      // The counts of available updates change (no fetch: the server compares with its cache)
      void storeUpdates.refresh(false);
    } catch (err) {
      onShowToast(errorOf(err, t('api.storeInstallFailed')), 'error');
    } finally {
      setInstalling(null);
    }
  };

  if (!canInstall) {
    return <p className={styles.notice}>{t('store.adminRequired')}</p>;
  }

  const q = query.trim().toLowerCase();
  const packages =
    state.status === 'ready'
      ? state.packages
          .filter((p) => p.kind === kind)
          .filter(
            (p) =>
              !q ||
              p.name.toLowerCase().includes(q) ||
              p.id.toLowerCase().includes(q) ||
              p.description.toLowerCase().includes(q)
          )
          .sort((a, b) => a.name.localeCompare(b.name))
      : [];
  // Recommended packages not installed yet come first (a new site has no plugin enabled)
  const recommended = packages.filter((p) => p.recommended && !p.installed_version);
  const others = packages.filter((p) => !recommended.includes(p));
  const failedCatalogs = state.status === 'ready' ? state.catalogs.filter((c) => !c.ok) : [];
  const confirmItems = confirm ? storeCapabilities(confirm.version.capabilities) : [];
  // An update: what the new version is allowed to do that the installed one was not
  const isUpdate = !!confirm?.pkg.installed_version;
  const addedItems =
    confirm && isUpdate
      ? capabilityAdditions(confirm.version.capabilities, confirm.pkg.installed_capabilities ?? {})
      : [];

  const card = (pkg: StorePackage) => {
    const latest = pkg.versions.find((v) => v.version === pkg.latest_compatible);
    const shown = pkg.latest_compatible ?? pkg.versions[0]?.version;
    const busy = installing === keyOf(pkg);
    const titleId = `store-${pkg.kind}-${pkg.catalog}-${pkg.id}`;
    return (
      <article key={keyOf(pkg)} className={styles.card} aria-labelledby={titleId}>
        <div className={styles.cardHead}>
          <h3 id={titleId} className={styles.name}>
            {pkg.name}
          </h3>
          {shown && <span className={styles.version}>v{shown}</span>}
        </div>
        <div className={styles.meta}>
          <code>{pkg.id}</code>
          {pkg.publisher === 'official' ? (
            <span className={styles.official}>
              <BadgeCheck size={13} aria-hidden="true" />
              {t('store.official')}
            </span>
          ) : (
            <span>{t('store.publisher', { name: pkg.publisher })}</span>
          )}
          {pkg.recommended && (
            <span className={styles.recommended}>
              <Star size={13} aria-hidden="true" />
              {t('store.recommended')}
            </span>
          )}
          {pkg.license && <span>{pkg.license}</span>}
        </div>
        <p className={styles.description}>{pkg.description}</p>
        <div className={styles.footer}>
          <span className={styles.status}>
            {pkg.installed_version
              ? pkg.update_available
                ? t('store.updateAvailable', {
                    installed: pkg.installed_version,
                    latest: pkg.latest_compatible,
                  })
                : pkg.installed_from_catalog
                  ? t('store.installed', { version: pkg.installed_version })
                  : t('store.installedElsewhere', { version: pkg.installed_version })
              : !latest &&
                t('store.incompatible', {
                  version: pkg.versions[0]?.min_aegis_version ?? '?',
                })}
          </span>
          {latest && (!pkg.installed_version || pkg.update_available) && (
            <Button
              variant={pkg.update_available ? 'secondary' : 'primary'}
              disabled={installing !== null}
              onClick={() => setConfirm({ pkg, version: latest })}
              aria-label={
                pkg.update_available
                  ? t('store.updateAria', { name: pkg.name, version: latest.version })
                  : t('store.installAria', { name: pkg.name })
              }
            >
              {busy ? <RefreshCw size={14} className="animate-spin" /> : <Download size={14} />}
              <span>
                {pkg.update_available
                  ? t('store.update', { version: latest.version })
                  : t('store.install')}
              </span>
            </Button>
          )}
        </div>
      </article>
    );
  };

  return (
    <section className={styles.panel} aria-label={t('store.title')}>
      <div className={styles.toolbar}>
        <p className={styles.intro}>{t('store.intro')}</p>
        <div className={styles.controls}>
          <label className={styles.search}>
            <Search size={15} aria-hidden="true" />
            <input
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t('store.searchPlaceholder')}
              aria-label={t('store.searchPlaceholder')}
            />
          </label>
          <Button
            variant="tool"
            onClick={refresh}
            disabled={state.status === 'loading'}
            aria-label={t('store.refresh')}
            title={t('store.refresh')}
          >
            <RefreshCw size={14} className={state.status === 'loading' ? 'animate-spin' : ''} />
          </Button>
        </div>
      </div>

      {failedCatalogs.map((c) => (
        <div key={c.name} className={styles.warning} role="alert">
          <AlertTriangle size={15} aria-hidden="true" />
          <span>
            {t('store.catalogFailed', {
              name: c.name,
              reason: errorText(c.code, c.error, t('api.storeFailed')),
            })}
          </span>
        </div>
      ))}

      {state.status === 'loading' && <p className={styles.notice}>{t('store.loading')}</p>}
      {state.status === 'disabled' && <p className={styles.notice}>{t('store.disabled')}</p>}
      {state.status === 'error' && (
        <div className={styles.warning} role="alert">
          <AlertTriangle size={15} aria-hidden="true" />
          <span>{state.message}</span>
        </div>
      )}
      {state.status === 'ready' && packages.length === 0 && (
        <p className={styles.notice}>{q ? t('store.noMatch') : t('store.empty')}</p>
      )}

      {recommended.length > 0 && (
        <section className={styles.group} aria-labelledby={`store-${kind}-recommended`}>
          <h3 id={`store-${kind}-recommended`} className={styles.groupTitle}>
            {t('store.recommendedTitle')}
          </h3>
          <p className={styles.notice}>{t('store.recommendedHint')}</p>
          <div className={styles.grid}>{recommended.map(card)}</div>
        </section>
      )}
      {others.length > 0 && (
        <div className={styles.group}>
          {recommended.length > 0 && <h3 className={styles.groupTitle}>{t('store.allTitle')}</h3>}
          <div className={styles.grid}>{others.map(card)}</div>
        </div>
      )}

      {confirm && (
        <Modal
          title={t('store.confirmTitle', {
            name: confirm.pkg.name,
            version: confirm.version.version,
          })}
          icon={<PackageSearch size={18} />}
          onClose={() => installing === null && setConfirm(null)}
        >
          <div className={styles.confirm}>
            <p>
              {t('store.confirmLead', {
                catalog: confirm.pkg.catalog,
                size: formatSize(confirm.version.size),
              })}
            </p>
            {confirm.pkg.installed_version && (
              <p>{t('store.confirmReplaces', { version: confirm.pkg.installed_version })}</p>
            )}
            {isUpdate && (
              <div>
                <h4 className={styles.capTitle}>{t('store.newCapabilitiesTitle')}</h4>
                {addedItems.length > 0 ? (
                  <ul
                    className={`${styles.capList} ${styles.capNew}`}
                    data-testid="store-capabilities-new"
                  >
                    {addedItems.map((item, i) => (
                      <li key={i}>{capabilityText(item, t)}</li>
                    ))}
                  </ul>
                ) : (
                  <p className={styles.muted}>{t('store.noNewCapabilities')}</p>
                )}
              </div>
            )}
            <div>
              <h4 className={styles.capTitle}>
                {isUpdate ? t('store.allCapabilitiesTitle') : t('store.capabilitiesTitle')}
              </h4>
              {confirmItems.length > 0 ? (
                <ul className={styles.capList} data-testid="store-capabilities">
                  {confirmItems.map((item, i) => (
                    <li key={i}>{capabilityText(item, t)}</li>
                  ))}
                </ul>
              ) : (
                <p className={styles.muted}>
                  {kind === 'plugin'
                    ? t('store.noCapabilitiesPlugin')
                    : t('store.noCapabilitiesTheme')}
                </p>
              )}
            </div>
            <p className={styles.muted}>
              {kind === 'plugin' ? t('store.afterInstallPlugin') : t('store.afterInstallTheme')}
            </p>
            <div className={styles.confirmActions}>
              <Button
                variant="tool"
                disabled={installing !== null}
                onClick={() => setConfirm(null)}
              >
                {t('common.cancel')}
              </Button>
              <Button variant="primary" disabled={installing !== null} onClick={install} autoFocus>
                {installing !== null ? (
                  <RefreshCw size={14} className="animate-spin" />
                ) : (
                  <Download size={14} />
                )}
                <span>
                  {installing !== null ? t('store.installing') : t('store.confirmInstall')}
                </span>
              </Button>
            </div>
          </div>
        </Modal>
      )}
    </section>
  );
};
