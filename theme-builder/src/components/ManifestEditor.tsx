import { useMemo, type KeyboardEvent } from 'react';
import { AlertCircle, Code, Info, Sparkles, User } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import {
  parseManifestJson,
  serializeManifestJson,
  type ManifestFormData,
} from '../manifestForm.ts';

export interface ManifestEditorProps {
  value: string;
  readOnly: boolean;
  onChange: (value: string) => void;
  onSave: () => void;
  onSwitchToJson: () => void;
}

export function ManifestEditor({
  value,
  readOnly,
  onChange,
  onSave,
  onSwitchToJson,
}: ManifestEditorProps) {
  const { t } = useTranslation();
  const parsed = useMemo(() => parseManifestJson(value), [value]);

  const handleKeyDown = (e: KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      onSave();
    }
  };

  if (!parsed.ok) {
    return (
      <div className="manifest-error" onKeyDown={handleKeyDown}>
        <div className="manifest-error-card">
          <AlertCircle size={24} className="error-icon" />
          <h3>{t('manifest.errorTitle')}</h3>
          <p>{parsed.error}</p>
          <p className="hint">{t('manifest.errorHint')}</p>
          <button type="button" className="primary" onClick={onSwitchToJson}>
            <Code size={14} /> {t('manifest.openInJson')}
          </button>
        </div>
      </div>
    );
  }

  const { data, raw } = parsed;

  const update = (patch: Partial<ManifestFormData>) => {
    const next: ManifestFormData = { ...data, ...patch };
    const serialized = serializeManifestJson(next, raw);
    onChange(serialized);
  };

  return (
    <div className="manifest-editor" onKeyDown={handleKeyDown}>
      <div className="manifest-container">
        {/* 基本情報 */}
        <section className="manifest-section">
          <div className="section-header">
            <Info size={16} />
            <h2>{t('manifest.basicInfo')}</h2>
          </div>
          <div className="form-grid">
            <div className="form-field">
              <label htmlFor="manifest-id">
                {t('manifest.themeId')} <span className="req">*</span>
              </label>
              <input
                id="manifest-id"
                value={data.id}
                disabled={readOnly}
                placeholder="my-theme"
                onChange={(e) => update({ id: e.target.value })}
                spellCheck={false}
              />
              <span className="field-hint">{t('manifest.themeIdHint')}</span>
            </div>

            <div className="form-field">
              <label htmlFor="manifest-name">
                {t('manifest.themeName')} <span className="req">*</span>
              </label>
              <input
                id="manifest-name"
                value={data.name}
                disabled={readOnly}
                placeholder="My Theme"
                onChange={(e) => update({ name: e.target.value })}
              />
              <span className="field-hint">{t('manifest.themeNameHint')}</span>
            </div>

            <div className="form-field">
              <label htmlFor="manifest-version">{t('manifest.version')}</label>
              <input
                id="manifest-version"
                value={data.version}
                disabled={readOnly}
                placeholder="1.0.0"
                onChange={(e) => update({ version: e.target.value })}
                spellCheck={false}
              />
              <span className="field-hint">{t('manifest.versionHint')}</span>
            </div>

            <div className="form-field full">
              <label htmlFor="manifest-desc">{t('manifest.description')}</label>
              <textarea
                id="manifest-desc"
                rows={3}
                value={data.description}
                disabled={readOnly}
                placeholder={t('manifest.descriptionPlaceholder')}
                onChange={(e) => update({ description: e.target.value })}
              />
            </div>
          </div>
        </section>

        {/* 作者情報 */}
        <section className="manifest-section">
          <div className="section-header">
            <User size={16} />
            <h2>{t('manifest.authorInfo')}</h2>
          </div>
          <div className="form-grid">
            <div className="form-field">
              <label htmlFor="manifest-author-name">{t('manifest.authorName')}</label>
              <input
                id="manifest-author-name"
                value={data.authorName}
                disabled={readOnly}
                placeholder="Author Name"
                onChange={(e) => update({ authorName: e.target.value })}
              />
            </div>

            <div className="form-field">
              <label htmlFor="manifest-author-email">{t('manifest.authorEmail')}</label>
              <input
                id="manifest-author-email"
                type="email"
                value={data.authorEmail}
                disabled={readOnly}
                placeholder="author@example.com"
                onChange={(e) => update({ authorEmail: e.target.value })}
                spellCheck={false}
              />
            </div>

            <div className="form-field full">
              <label htmlFor="manifest-author-url">{t('manifest.authorUrl')}</label>
              <input
                id="manifest-author-url"
                type="url"
                value={data.authorUrl}
                disabled={readOnly}
                placeholder="https://example.com"
                onChange={(e) => update({ authorUrl: e.target.value })}
                spellCheck={false}
              />
            </div>
          </div>
        </section>

        {/* 配布・メタデータ */}
        <section className="manifest-section">
          <div className="section-header">
            <Sparkles size={16} />
            <h2>{t('manifest.distributionMeta')}</h2>
          </div>
          <div className="form-grid">
            <div className="form-field">
              <label htmlFor="manifest-license">{t('manifest.license')}</label>
              <input
                id="manifest-license"
                value={data.license}
                disabled={readOnly}
                placeholder="MIT"
                onChange={(e) => update({ license: e.target.value })}
                spellCheck={false}
              />
            </div>

            <div className="form-field">
              <label htmlFor="manifest-preview">{t('manifest.previewImage')}</label>
              <input
                id="manifest-preview"
                value={data.previewImage}
                disabled={readOnly}
                placeholder="preview.svg"
                onChange={(e) => update({ previewImage: e.target.value })}
                spellCheck={false}
              />
              <span className="field-hint">{t('manifest.previewImageHint')}</span>
            </div>

            <div className="form-field full">
              <label htmlFor="manifest-tags">{t('manifest.tags')}</label>
              <input
                id="manifest-tags"
                value={data.tags}
                disabled={readOnly}
                placeholder="dark, minimal, blog, official"
                onChange={(e) => update({ tags: e.target.value })}
              />
              <span className="field-hint">{t('manifest.tagsHint')}</span>
            </div>

            <div className="form-field">
              <label htmlFor="manifest-homepage">{t('manifest.homepage')}</label>
              <input
                id="manifest-homepage"
                type="url"
                value={data.homepage}
                disabled={readOnly}
                placeholder="https://example.com/theme"
                onChange={(e) => update({ homepage: e.target.value })}
                spellCheck={false}
              />
            </div>

            <div className="form-field">
              <label htmlFor="manifest-repo">{t('manifest.repository')}</label>
              <input
                id="manifest-repo"
                type="url"
                value={data.repository}
                disabled={readOnly}
                placeholder="https://github.com/example/theme"
                onChange={(e) => update({ repository: e.target.value })}
                spellCheck={false}
              />
            </div>

            <div className="form-field">
              <label htmlFor="manifest-min-version">{t('manifest.minAegisVersion')}</label>
              <input
                id="manifest-min-version"
                value={data.minAegisVersion}
                disabled={readOnly}
                placeholder="0.1.0"
                onChange={(e) => update({ minAegisVersion: e.target.value })}
                spellCheck={false}
              />
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
