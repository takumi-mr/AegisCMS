import React, { useMemo, useState } from 'react';
import { ExternalLink, Scale, Search, ShieldCheck } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import licenseData from '../data/licenses.json';
import styles from './LicenseManager.module.css';

interface PackageItem {
  name: string;
  version: string;
  license: string;
  repository: string | null;
  description?: string | null;
  usedIn?: string;
  category: 'rust' | 'npm';
}

type FilterCategory = 'all' | 'rust' | 'npm';

export const LicenseManager: React.FC = () => {
  const { t } = useTranslation();
  const [filter, setFilter] = useState<FilterCategory>('all');
  const [searchQuery, setSearchQuery] = useState('');

  const allPackages = useMemo<PackageItem[]>(() => {
    const rustList: PackageItem[] = (licenseData.rust || []).map((item) => ({
      ...item,
      category: 'rust',
    }));
    const npmList: PackageItem[] = (licenseData.npm || []).map((item) => ({
      ...item,
      category: 'npm',
    }));
    return [...rustList, ...npmList];
  }, []);

  const filteredPackages = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    return allPackages.filter((pkg) => {
      if (filter !== 'all' && pkg.category !== filter) {
        return false;
      }
      if (!query) {
        return true;
      }
      const matchName = pkg.name.toLowerCase().includes(query);
      const matchLicense = pkg.license.toLowerCase().includes(query);
      const matchDesc = pkg.description?.toLowerCase().includes(query) ?? false;
      return matchName || matchLicense || matchDesc;
    });
  }, [allPackages, filter, searchQuery]);

  return (
    <main className={styles.main} role="region" aria-label={t('licenses.title')}>
      <div className={styles.column}>
        <div className={styles.headerRow}>
          <div className={styles.titleArea}>
            <div className={styles.title}>
              <Scale size={24} />
              <h1>{t('licenses.title')}</h1>
            </div>
            <p className={styles.description}>{t('licenses.description')}</p>
          </div>
        </div>

        {/* AegisCMS Core License Banner */}
        <section className={styles.aegisCard} aria-labelledby="aegis-license-heading">
          <div className={styles.aegisCardHeader}>
            <div className={styles.aegisCardTitle} id="aegis-license-heading">
              <ShieldCheck size={20} color="var(--accent-indigo-light, #818cf8)" />
              <span>{t('licenses.aegisLicenseTitle')}</span>
            </div>
            <span className={`${styles.badge} ${styles.badgeSuccess}`}>MIT License</span>
          </div>
          <p className={styles.aegisCardText}>
            {t('licenses.aegisLicenseBody')}
            <br />
            Copyright (c) 2026 AegisCMS Team.
          </p>
        </section>

        {/* Filter & Search Toolbar */}
        <div className={styles.toolbar}>
          <div className={styles.tabs} role="tablist" aria-label="Package category filter">
            <button
              type="button"
              role="tab"
              aria-selected={filter === 'all'}
              className={`${styles.tab} ${filter === 'all' ? styles.tabActive : ''}`}
              onClick={() => setFilter('all')}
            >
              {t('licenses.filterAll')}
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={filter === 'rust'}
              className={`${styles.tab} ${filter === 'rust' ? styles.tabActive : ''}`}
              onClick={() => setFilter('rust')}
            >
              {t('licenses.filterRust')}
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={filter === 'npm'}
              className={`${styles.tab} ${filter === 'npm' ? styles.tabActive : ''}`}
              onClick={() => setFilter('npm')}
            >
              {t('licenses.filterNpm')}
            </button>
          </div>

          <div className={styles.searchWrapper}>
            <Search size={16} className={styles.searchIcon} />
            <input
              type="search"
              className={styles.searchInput}
              placeholder={t('licenses.searchPlaceholder')}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              aria-label={t('licenses.searchPlaceholder')}
            />
          </div>
        </div>

        <div className={styles.metaInfo}>
          <span>{t('licenses.packageCount', { count: filteredPackages.length })}</span>
        </div>

        {/* Package List */}
        {filteredPackages.length === 0 ? (
          <div className={styles.emptyState}>
            <p>{t('licenses.noResults')}</p>
          </div>
        ) : (
          <div className={styles.packageList}>
            {filteredPackages.map((pkg) => {
              const cleanRepo = pkg.repository
                ? pkg.repository.replace(/^git\+/, '').replace(/\.git$/, '')
                : null;

              return (
                <article
                  key={`${pkg.category}:${pkg.name}@${pkg.version}`}
                  className={styles.packageCard}
                >
                  <div className={styles.packageHeader}>
                    <div className={styles.packageNameArea}>
                      <span className={styles.packageName}>{pkg.name}</span>
                      <span className={styles.packageVersion}>v{pkg.version}</span>
                    </div>

                    <div className={styles.packageTags}>
                      <span className={styles.tagCategory}>
                        {pkg.category === 'rust' ? 'Rust' : 'npm'}
                        {pkg.usedIn ? ` (${pkg.usedIn})` : ''}
                      </span>
                      <span className={styles.badge}>{pkg.license}</span>
                    </div>
                  </div>

                  {pkg.description && (
                    <p className={styles.packageDescription}>{pkg.description}</p>
                  )}

                  {cleanRepo && (
                    <a
                      href={cleanRepo}
                      target="_blank"
                      rel="noopener noreferrer"
                      className={styles.repoLink}
                    >
                      <ExternalLink size={13} />
                      <span>{t('licenses.repository')}</span>
                    </a>
                  )}
                </article>
              );
            })}
          </div>
        )}
      </div>
    </main>
  );
};
