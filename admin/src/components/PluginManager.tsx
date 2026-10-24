import React, { useState, useEffect, useMemo } from 'react';
import {
  Blocks,
  CheckCircle,
  AlertCircle,
  ExternalLink,
  Sparkles,
  ShoppingBag,
  Search,
  User,
  ShieldCheck,
  RefreshCw,
  Settings,
  Database,
  Globe,
  Clock,
  Cpu,
  Eye,
  EyeOff,
  Trash2,
  Plus,
  X,
  Lock,
  Power,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import i18n from '../i18n';
import type { PackageAuthor, PluginInfo, SessionInfo } from '../types';
import { isAdmin } from '../utils/roles';
import { cx } from '../utils/cx';
import styles from './PluginManager.module.css';

interface PluginManagerProps {
  session: SessionInfo;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/** Suggested keys for known official plugins to provide a guided configuration experience */
const KNOWN_PLUGIN_PRESETS: Record<string, string[]> = {
  'official-stripe': ['secret_key', 'publishable_key', 'webhook_secret'],
  'official-resend': ['api_key', 'from_email'],
  'official-turnstile': ['site_key', 'secret_key'],
  'official-order-notifier': ['channel_name', 'webhook_url'],
  'official-calc': ['tax_rate', 'free_shipping_threshold'],
};

const authorName = (author: PackageAuthor) => (typeof author === 'string' ? author : author.name);

export const PluginManager: React.FC<PluginManagerProps> = ({ session, onShowToast }) => {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<'installed' | 'market'>('installed');
  const [plugins, setPlugins] = useState<PluginInfo[]>([]);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [reloadTrigger, setReloadTrigger] = useState<number>(0);
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [statusFilter, setStatusFilter] = useState<'all' | 'enabled' | 'disabled'>('all');

  // Active modal plugin
  const [editingPlugin, setEditingPlugin] = useState<PluginInfo | null>(null);
  const [currentSettings, setCurrentSettings] = useState<Record<string, string>>({});
  const [newKey, setNewKey] = useState<string>('');
  const [newValue, setNewValue] = useState<string>('');
  const [isSavingSettings, setIsSavingSettings] = useState<boolean>(false);
  const [revealedKeys, setRevealedKeys] = useState<Set<string>>(new Set());
  // Plugin whose enable / disable request is in flight
  const [togglingName, setTogglingName] = useState<string | null>(null);

  const canConfigure = isAdmin(session);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listPlugins()
      .then((data) => {
        if (!cancelled) {
          setPlugins(data);
          setIsLoading(false);
        }
      })
      .catch((err: unknown) => {
        console.error('Failed to load plugins:', err);
        if (!cancelled) {
          onShowToast(i18n.t('plugins.loadFailed'), 'error');
          setIsLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [reloadTrigger, onShowToast]);

  const reloadPlugins = () => {
    setIsLoading(true);
    setReloadTrigger((n) => n + 1);
  };

  const handleOpenSettings = (plugin: PluginInfo) => {
    setEditingPlugin(plugin);
    setCurrentSettings({ ...(plugin.settings ?? {}) });
    setNewKey('');
    setNewValue('');
    setRevealedKeys(new Set());
  };

  const handleCloseSettings = () => {
    setEditingPlugin(null);
    setCurrentSettings({});
    setNewKey('');
    setNewValue('');
  };

  const handleToggleReveal = (key: string) => {
    setRevealedKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  const handleAddSetting = () => {
    const trimmedKey = newKey.trim();
    if (!trimmedKey) return;
    setCurrentSettings((prev) => ({
      ...prev,
      [trimmedKey]: newValue,
    }));
    setNewKey('');
    setNewValue('');
  };

  const handleDeleteSetting = (key: string) => {
    setCurrentSettings((prev) => {
      const next = { ...prev };
      delete next[key];
      return next;
    });
  };

  const handleSettingChange = (key: string, value: string) => {
    setCurrentSettings((prev) => ({
      ...prev,
      [key]: value,
    }));
  };

  const handleSaveSettings = async () => {
    if (!editingPlugin) return;
    if (!canConfigure) {
      onShowToast(t('plugins.adminRequired'), 'error');
      return;
    }

    setIsSavingSettings(true);
    try {
      const updated = await aegisApi.updatePluginSettings(editingPlugin.name, currentSettings);
      onShowToast(t('plugins.settingsSaved', { name: editingPlugin.name }), 'success');
      // Update local state
      setPlugins((prev) =>
        prev.map((p) => (p.name === editingPlugin.name ? { ...p, settings: updated } : p))
      );
      handleCloseSettings();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : t('plugins.saveFailed');
      onShowToast(msg, 'error');
    } finally {
      setIsSavingSettings(false);
    }
  };

  const handleToggleEnabled = async (plugin: PluginInfo) => {
    if (!canConfigure) {
      onShowToast(t('plugins.toggleAdminRequired'), 'error');
      return;
    }
    const enable = !plugin.is_enabled;
    if (!enable && !window.confirm(t('plugins.confirmDisable', { name: plugin.name }))) {
      return;
    }

    setTogglingName(plugin.name);
    try {
      setPlugins(await aegisApi.setPluginEnabled(plugin.name, enable));
      onShowToast(
        t(enable ? 'plugins.enabled' : 'plugins.disabled', { name: plugin.name }),
        'success'
      );
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : t('api.setPluginEnabledFailed');
      onShowToast(msg, 'error');
    } finally {
      setTogglingName(null);
    }
  };

  const isSensitiveKey = (key: string) => {
    const lower = key.toLowerCase();
    return (
      lower.includes('secret') ||
      lower.includes('key') ||
      lower.includes('token') ||
      lower.includes('password')
    );
  };

  const filteredPlugins = useMemo(() => {
    const q = searchQuery.toLowerCase().trim();
    return plugins.filter((plugin) => {
      if (statusFilter === 'enabled' && !plugin.is_enabled) return false;
      if (statusFilter === 'disabled' && plugin.is_enabled) return false;
      if (!q) return true;

      const manifest = plugin.manifest;
      const displayName = manifest?.display_name?.toLowerCase() || '';
      const name = plugin.name.toLowerCase();
      const desc = manifest?.description?.toLowerCase() || '';
      const hooksMatch = (manifest?.capabilities.hooks ?? []).some((h) =>
        h.toLowerCase().includes(q)
      );

      return name.includes(q) || displayName.includes(q) || desc.includes(q) || hooksMatch;
    });
  }, [plugins, searchQuery, statusFilter]);

  const presetSuggestions = editingPlugin ? KNOWN_PLUGIN_PRESETS[editingPlugin.name] || [] : [];

  return (
    <main className={styles.main}>
      <div className={styles.container}>
        <div className={styles.header}>
          <div className={styles.titleRow}>
            <h1 className={styles.title}>
              <Blocks className={styles.titleIcon} size={28} />
              <span>{t('plugins.title')}</span>
            </h1>

            <button
              onClick={reloadPlugins}
              disabled={isLoading}
              className={styles.settingsBtn}
              style={{ width: 'auto', padding: '0.6rem 1rem' }}
              title={t('plugins.refreshTitle')}
            >
              <RefreshCw size={14} className={isLoading ? 'animate-spin' : ''} />
              <span>{t('plugins.refresh')}</span>
            </button>
          </div>

          <p className={styles.description}>{t('plugins.description')}</p>

          <div className={styles.tabs}>
            <button
              className={cx(styles.tab, activeTab === 'installed' && styles.tabActive)}
              onClick={() => setActiveTab('installed')}
            >
              <Sparkles size={16} />
              <span>{t('plugins.installed')}</span>
              <span className={styles.tabBadge}>{plugins.length}</span>
            </button>
            <button
              className={cx(styles.tab, activeTab === 'market' && styles.tabActive)}
              onClick={() => setActiveTab('market')}
            >
              <ShoppingBag size={16} />
              <span>{t('plugins.ecosystem')}</span>
              <span className={styles.tabBadge}>Zero-Trust</span>
            </button>
          </div>
        </div>

        {activeTab === 'installed' && (
          <>
            <div className={styles.filterBar}>
              <div className={styles.searchBox}>
                <Search size={16} className={styles.searchIcon} />
                <input
                  type="text"
                  placeholder={t('plugins.searchPlaceholder')}
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className={styles.searchInput}
                />
              </div>

              <div className={styles.statusFilter}>
                <button
                  type="button"
                  className={cx(styles.statusBtn, statusFilter === 'all' && styles.statusBtnActive)}
                  onClick={() => setStatusFilter('all')}
                >
                  {t('plugins.filterAll')}
                </button>
                <button
                  type="button"
                  className={cx(
                    styles.statusBtn,
                    statusFilter === 'enabled' && styles.statusBtnActive
                  )}
                  onClick={() => setStatusFilter('enabled')}
                >
                  {t('plugins.filterEnabled')}
                </button>
                <button
                  type="button"
                  className={cx(
                    styles.statusBtn,
                    statusFilter === 'disabled' && styles.statusBtnActive
                  )}
                  onClick={() => setStatusFilter('disabled')}
                >
                  {t('plugins.filterDisabled')}
                </button>
              </div>
            </div>

            {filteredPlugins.length === 0 ? (
              <div className={styles.emptyState}>
                <Blocks size={48} className={styles.emptyIcon} />
                <h3 className={styles.emptyTitle}>{t('plugins.emptyTitle')}</h3>
                <p className={styles.emptyDesc}>{t('plugins.emptyDesc')}</p>
              </div>
            ) : (
              <div className={styles.grid}>
                {filteredPlugins.map((plugin) => {
                  const m = plugin.manifest;
                  const displayName = m?.display_name || plugin.name;
                  const settingsCount = Object.keys(plugin.settings || {}).length;
                  const hooks = m?.capabilities.hooks ?? [];
                  const allowedDomains = m?.capabilities.network?.allowed_domains ?? [];
                  const limits = m?.resource_limits;
                  const isToggling = togglingName === plugin.name;
                  // A broken plugin can still be disabled, but not enabled
                  const canToggle =
                    canConfigure && togglingName === null && (plugin.is_enabled || !plugin.problem);

                  return (
                    <div
                      key={plugin.name}
                      className={cx(styles.card, plugin.is_enabled && styles.cardEnabled)}
                    >
                      <div className={styles.cardBody}>
                        <div className={styles.cardTop}>
                          <div className={styles.pluginIconWrapper}>
                            <Blocks size={24} />
                          </div>
                          <div className={styles.cardTopInfo}>
                            <div className={styles.nameRow}>
                              <h2 className={styles.pluginName} title={displayName}>
                                {displayName}
                              </h2>
                              {m && <span className={styles.version}>v{m.version}</span>}
                            </div>
                            <div className={styles.idRow}>
                              <span className={styles.pluginId}>{plugin.name}</span>
                              {plugin.problem ? (
                                <span className={cx(styles.statusBadge, styles.badgeError)}>
                                  <AlertCircle size={12} />
                                  <span>{t('plugins.statusError')}</span>
                                </span>
                              ) : plugin.is_enabled ? (
                                <span className={cx(styles.statusBadge, styles.badgeEnabled)}>
                                  <CheckCircle size={12} />
                                  <span>{t('plugins.statusEnabled')}</span>
                                </span>
                              ) : (
                                <span className={cx(styles.statusBadge, styles.badgeDisabled)}>
                                  <span>{t('plugins.statusDisabled')}</span>
                                </span>
                              )}
                            </div>
                          </div>
                        </div>

                        <p className={styles.pluginDesc}>
                          {m?.description ||
                            (plugin.is_installed
                              ? t('plugins.noDescription')
                              : t('plugins.notInstalled'))}
                        </p>

                        {plugin.problem && (
                          <div className={styles.errorBox}>
                            <AlertCircle size={14} style={{ flexShrink: 0, marginTop: 2 }} />
                            <span>{plugin.problem}</span>
                          </div>
                        )}

                        {hooks.length > 0 && (
                          <div className={styles.hooksSection}>
                            <span className={styles.sectionLabel}>{t('plugins.hooksLabel')}</span>
                            <div className={styles.hooksList}>
                              {hooks.map((hook) => (
                                <span key={hook} className={styles.hookTag}>
                                  {hook}
                                </span>
                              ))}
                            </div>
                          </div>
                        )}

                        {m && (
                          <div className={styles.capabilitiesSection}>
                            <span className={styles.sectionLabel}>
                              {t('plugins.capabilitiesLabel')}
                            </span>
                            <div className={styles.capItem}>
                              <Database size={13} className={styles.capItemIcon} />
                              <span className={styles.capText}>
                                {m.capabilities.storage
                                  ? t('plugins.kvStorageEnabled')
                                  : t('plugins.kvStorageDisabled')}
                              </span>
                            </div>
                            {allowedDomains.length > 0 && (
                              <div className={styles.capItem}>
                                <Globe size={13} className={styles.capItemIcon} />
                                <span className={styles.capText} title={allowedDomains.join(', ')}>
                                  {t('plugins.allowedDomains', {
                                    domains: allowedDomains.join(', '),
                                  })}
                                </span>
                              </div>
                            )}
                            {limits && (
                              <div className={styles.capItem}>
                                <Cpu size={13} className={styles.capItemIcon} />
                                <span className={styles.capText}>
                                  {`${limits.max_memory_mb}MB (${limits.execution_timeout_ms}ms)`}
                                </span>
                              </div>
                            )}
                          </div>
                        )}

                        <div className={styles.metaRow}>
                          <div className={styles.metaLeft}>
                            {m?.author && (
                              <div className={styles.metaItem}>
                                <User size={13} />
                                <span>{authorName(m.author)}</span>
                              </div>
                            )}
                            {m?.license && (
                              <div className={styles.metaItem}>
                                <ShieldCheck size={13} />
                                <span>{m.license}</span>
                              </div>
                            )}
                          </div>

                          {m?.homepage && (
                            <div className={styles.metaItem}>
                              <a
                                href={m.homepage}
                                target="_blank"
                                rel="noopener noreferrer"
                                title={t('plugins.viewDocs')}
                              >
                                <ExternalLink size={13} />
                              </a>
                            </div>
                          )}
                        </div>

                        <div className={styles.cardActions}>
                          <button
                            type="button"
                            className={cx(
                              styles.toggleBtn,
                              plugin.is_enabled ? styles.toggleBtnDisable : styles.toggleBtnEnable
                            )}
                            onClick={() => handleToggleEnabled(plugin)}
                            disabled={!canToggle}
                            title={
                              plugin.is_enabled
                                ? t('plugins.disableTitle')
                                : plugin.problem || t('plugins.enableTitle')
                            }
                          >
                            {isToggling ? (
                              <RefreshCw size={15} className="animate-spin" />
                            ) : (
                              <Power size={15} />
                            )}
                            <span>
                              {plugin.is_enabled ? t('plugins.disable') : t('plugins.enable')}
                            </span>
                          </button>
                          <button
                            className={styles.settingsBtn}
                            onClick={() => handleOpenSettings(plugin)}
                            title={t('plugins.configureTitle')}
                          >
                            <Settings size={15} />
                            <span>{t('plugins.configure')}</span>
                            {settingsCount > 0 && (
                              <span className={styles.settingsCount}>
                                {t('plugins.settingsCount', { count: settingsCount })}
                              </span>
                            )}
                          </button>
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </>
        )}

        {activeTab === 'market' && (
          <div className={styles.ecoHero}>
            <h2 className={styles.ecoHeroTitle}>{t('plugins.ecoTitle')}</h2>
            <p className={styles.description}>{t('plugins.ecoDesc')}</p>

            <div className={styles.ecoFeatures}>
              <div className={styles.ecoFeatureCard}>
                <ShieldCheck size={28} className={styles.ecoFeatureIcon} />
                <h3 className={styles.ecoFeatureTitle}>{t('plugins.featSandboxTitle')}</h3>
                <p className={styles.ecoFeatureDesc}>{t('plugins.featSandboxDesc')}</p>
              </div>

              <div className={styles.ecoFeatureCard}>
                <Globe size={28} className={styles.ecoFeatureIcon} />
                <h3 className={styles.ecoFeatureTitle}>{t('plugins.featCapabilityTitle')}</h3>
                <p className={styles.ecoFeatureDesc}>{t('plugins.featCapabilityDesc')}</p>
              </div>

              <div className={styles.ecoFeatureCard}>
                <Clock size={28} className={styles.ecoFeatureIcon} />
                <h3 className={styles.ecoFeatureTitle}>{t('plugins.featSpeedTitle')}</h3>
                <p className={styles.ecoFeatureDesc}>{t('plugins.featSpeedDesc')}</p>
              </div>
            </div>

            <div className={styles.ecoOfficialList}>
              <h3 className={styles.ecoFeatureTitle}>{t('plugins.officialPluginsTitle')}</h3>

              <div className={styles.ecoOfficialItem}>
                <div className={styles.ecoOfficialLeft}>
                  <Blocks size={24} color="#6366f1" />
                  <div>
                    <div className={styles.ecoOfficialName}>official-stripe</div>
                    <div className={styles.ecoOfficialDesc}>{t('plugins.officialStripeDesc')}</div>
                  </div>
                </div>
                <span className={styles.hookTag}>calculate-order, order-status</span>
              </div>

              <div className={styles.ecoOfficialItem}>
                <div className={styles.ecoOfficialLeft}>
                  <Blocks size={24} color="#10b981" />
                  <div>
                    <div className={styles.ecoOfficialName}>official-resend</div>
                    <div className={styles.ecoOfficialDesc}>{t('plugins.officialResendDesc')}</div>
                  </div>
                </div>
                <span className={styles.hookTag}>notify-event</span>
              </div>

              <div className={styles.ecoOfficialItem}>
                <div className={styles.ecoOfficialLeft}>
                  <Blocks size={24} color="#f59e0b" />
                  <div>
                    <div className={styles.ecoOfficialName}>official-turnstile</div>
                    <div className={styles.ecoOfficialDesc}>
                      {t('plugins.officialTurnstileDesc')}
                    </div>
                  </div>
                </div>
                <span className={styles.hookTag}>validate-submission</span>
              </div>
            </div>
          </div>
        )}
      </div>

      {/* Settings Modal */}
      {editingPlugin && (
        <div className={styles.modalBackdrop} onClick={handleCloseSettings}>
          <div className={styles.modalContent} onClick={(e) => e.stopPropagation()}>
            <div className={styles.modalHeader}>
              <div className={styles.modalTitleGroup}>
                <Settings size={20} className={styles.titleIcon} />
                <h2 className={styles.modalTitle}>
                  {t('plugins.settingsModalTitle', {
                    name: editingPlugin.manifest?.display_name || editingPlugin.name,
                  })}
                </h2>
              </div>
              <button
                className={styles.closeBtn}
                onClick={handleCloseSettings}
                aria-label={t('common.close', 'Close')}
              >
                <X size={18} />
              </button>
            </div>

            <div className={styles.modalBody}>
              <div className={styles.modalNotice}>{t('plugins.settingsNotice')}</div>

              {presetSuggestions.length > 0 && (
                <div className={styles.presetSection}>
                  <span className={styles.presetTitle}>{t('plugins.presetSuggestions')}</span>
                  <div className={styles.presetPills}>
                    {presetSuggestions.map((key) => (
                      <button
                        key={key}
                        type="button"
                        className={styles.presetPill}
                        onClick={() => {
                          setNewKey(key);
                        }}
                        title={t('plugins.applyPresetKey', { key })}
                      >
                        + {key}
                      </button>
                    ))}
                  </div>
                </div>
              )}

              <div className={styles.settingsTable}>
                <span className={styles.sectionLabel}>{t('plugins.activeSettingsLabel')}</span>

                {Object.keys(currentSettings).length === 0 ? (
                  <p style={{ color: 'var(--text-dim)', fontSize: '0.85rem', fontStyle: 'italic' }}>
                    {t('plugins.noSettingsConfigured')}
                  </p>
                ) : (
                  Object.entries(currentSettings).map(([key, value]) => {
                    const sensitive = isSensitiveKey(key);
                    const revealed = revealedKeys.has(key);

                    return (
                      <div key={key} className={styles.settingRow}>
                        <div className={styles.settingKey} title={key}>
                          {key}
                        </div>
                        <div className={styles.settingValueWrapper}>
                          <input
                            type={sensitive && !revealed ? 'password' : 'text'}
                            value={value}
                            onChange={(e) => handleSettingChange(key, e.target.value)}
                            className={styles.settingValueInput}
                            placeholder={t('plugins.valuePlaceholder')}
                          />
                          {sensitive && (
                            <button
                              type="button"
                              className={styles.visibilityToggle}
                              onClick={() => handleToggleReveal(key)}
                              title={revealed ? t('plugins.hideValue') : t('plugins.showValue')}
                            >
                              {revealed ? <EyeOff size={14} /> : <Eye size={14} />}
                            </button>
                          )}
                        </div>
                        <button
                          type="button"
                          className={styles.deleteRowBtn}
                          onClick={() => handleDeleteSetting(key)}
                          title={t('plugins.deleteSetting')}
                        >
                          <Trash2 size={15} />
                        </button>
                      </div>
                    );
                  })
                )}
              </div>

              <div className={styles.addSettingForm}>
                <input
                  type="text"
                  placeholder={t('plugins.newKeyPlaceholder')}
                  value={newKey}
                  onChange={(e) => setNewKey(e.target.value)}
                  className={cx(styles.inputField, styles.keyInput)}
                />
                <input
                  type="text"
                  placeholder={t('plugins.newValuePlaceholder')}
                  value={newValue}
                  onChange={(e) => setNewValue(e.target.value)}
                  className={cx(styles.inputField, styles.valInput)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      e.preventDefault();
                      handleAddSetting();
                    }
                  }}
                />
                <button
                  type="button"
                  className={styles.addRowBtn}
                  onClick={handleAddSetting}
                  disabled={!newKey.trim()}
                >
                  <Plus size={14} />
                  <span>{t('plugins.addSetting')}</span>
                </button>
              </div>
            </div>

            <div className={styles.modalFooter}>
              <button
                type="button"
                className={styles.cancelBtn}
                onClick={handleCloseSettings}
                disabled={isSavingSettings}
              >
                {t('common.cancel', 'Cancel')}
              </button>
              <button
                type="button"
                className={styles.saveBtn}
                onClick={handleSaveSettings}
                disabled={isSavingSettings || !canConfigure}
              >
                {isSavingSettings ? (
                  <>
                    <RefreshCw size={14} className="animate-spin" />
                    <span>{t('plugins.saving')}</span>
                  </>
                ) : (
                  <>
                    <Lock size={14} />
                    <span>{t('plugins.saveSettings')}</span>
                  </>
                )}
              </button>
            </div>
          </div>
        </div>
      )}
    </main>
  );
};
