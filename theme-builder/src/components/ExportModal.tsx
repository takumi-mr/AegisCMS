import { useEffect, useState } from 'react';
import {
  AlertCircle,
  Check,
  CheckCircle2,
  Copy,
  Download,
  Key,
  Package,
  ShieldCheck,
  X,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { api } from '../api.ts';
import type { PackageValidationResult } from '../types.ts';

export interface ExportModalProps {
  isOpen: boolean;
  hasDirtyFiles: boolean;
  onClose: () => void;
}

export function ExportModal({ isOpen, hasDirtyFiles, onClose }: ExportModalProps) {
  const { t } = useTranslation();
  const [validation, setValidation] = useState<PackageValidationResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [signing, setSigning] = useState(false);
  const [customKey, setCustomKey] = useState('');
  const [showKeyInput, setShowKeyInput] = useState(false);
  const [generatedPrivKey, setGeneratedPrivKey] = useState<string | null>(null);
  const [copiedKey, setCopiedKey] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const fetchValidation = async () => {
    setLoading(true);
    setErrorMsg(null);
    try {
      const res = await api.packageValidate();
      setValidation(res);
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!isOpen) return;
    let ignore = false;
    api
      .packageValidate()
      .then((res) => {
        if (!ignore) {
          setValidation(res);
          setErrorMsg(null);
        }
      })
      .catch((e) => {
        if (!ignore) {
          setErrorMsg(e instanceof Error ? e.message : String(e));
        }
      });
    return () => {
      ignore = true;
    };
  }, [isOpen]);

  if (!isOpen) return null;

  const handleSign = async (generate: boolean) => {
    setSigning(true);
    setErrorMsg(null);
    try {
      const res = await api.packageSign({
        generate_key: generate,
        private_key: generate ? undefined : customKey.trim(),
      });
      if (res.generated_private_key) {
        setGeneratedPrivKey(res.generated_private_key);
      }
      await fetchValidation();
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
    } finally {
      setSigning(false);
    }
  };

  const handleCopyPrivKey = () => {
    if (generatedPrivKey) {
      void navigator.clipboard.writeText(generatedPrivKey);
      setCopiedKey(true);
      setTimeout(() => setCopiedKey(false), 2000);
    }
  };

  const manifest = validation?.manifest;
  const filename = manifest
    ? `${manifest.id}-${manifest.version || '1.0.0'}.zip`
    : 'theme-package.zip';

  return (
    <div className="modal-backdrop" onClick={onClose} role="dialog" aria-modal="true">
      <div
        className="modal-card export-modal"
        onClick={(e) => e.stopPropagation()}
        aria-label={t('export.title')}
      >
        <div className="modal-header">
          <div className="modal-title">
            <Package size={18} />
            <h2>{t('export.title')}</h2>
          </div>
          <button
            type="button"
            className="icon-btn"
            onClick={onClose}
            aria-label={t('common.close')}
          >
            <X size={16} />
          </button>
        </div>

        <div className="modal-body">
          {hasDirtyFiles && (
            <div className="notice warning" role="alert">
              <AlertCircle size={15} />
              <span>{t('export.unsavedWarning')}</span>
            </div>
          )}

          {errorMsg && (
            <div className="notice danger" role="alert">
              <AlertCircle size={15} />
              <span>{errorMsg}</span>
            </div>
          )}

          {/* 1. パッケージ検証ステータス */}
          <section className="export-section">
            <div className="section-title">
              <h3>{t('export.validationTitle')}</h3>
              {loading && <span className="loading-hint">{t('export.validating')}</span>}
            </div>

            {validation && (
              <div className="validation-box">
                <ul className="check-list">
                  <li className={manifest ? 'pass' : 'fail'}>
                    {manifest ? <CheckCircle2 size={16} /> : <AlertCircle size={16} />}
                    <span>
                      {t('export.themeJson')}{' '}
                      {manifest ? `${manifest.name} (${manifest.id})` : t('export.notFound')}
                    </span>
                  </li>
                  <li
                    className={
                      validation.errors.some((e) => e.includes('base.html')) ? 'fail' : 'pass'
                    }
                  >
                    {!validation.errors.some((e) => e.includes('base.html')) ? (
                      <CheckCircle2 size={16} />
                    ) : (
                      <AlertCircle size={16} />
                    )}
                    <span>{t('export.requiredTemplate')}</span>
                  </li>
                  <li className={validation.signed ? 'pass' : 'info'}>
                    {validation.signed ? <ShieldCheck size={16} /> : <Key size={16} />}
                    <span>
                      {t('export.digitalSignature')}{' '}
                      {validation.signed ? t('export.signedValid') : t('export.unsigned')}
                    </span>
                  </li>
                </ul>

                {validation.errors.length > 0 && (
                  <div className="validation-errors">
                    {validation.errors.map((err, i) => (
                      <p key={i} className="error-item">
                        {err}
                      </p>
                    ))}
                  </div>
                )}
              </div>
            )}
          </section>

          {/* 2. デジタル署名 */}
          <section className="export-section">
            <div className="section-title">
              <h3>{t('export.signatureTitle')}</h3>
              {validation?.signed && (
                <span className="badge success">{t('export.signedBadge')}</span>
              )}
            </div>
            <p className="section-desc">{t('export.signatureDesc')}</p>

            {validation?.public_key && (
              <div className="pubkey-box">
                <span className="label">{t('export.publicKey')}</span>
                <code className="pubkey">{validation.public_key}</code>
              </div>
            )}

            {generatedPrivKey && (
              <div className="generated-key-alert">
                <div className="alert-header">
                  <Key size={15} />
                  <strong>{t('export.newPrivateKey')}</strong>
                  <button
                    type="button"
                    className="copy-btn"
                    onClick={handleCopyPrivKey}
                    title={t('export.copyKeyTitle')}
                  >
                    {copiedKey ? <Check size={13} /> : <Copy size={13} />}
                    <span>{copiedKey ? t('export.copied') : t('export.copy')}</span>
                  </button>
                </div>
                <code className="privkey">{generatedPrivKey}</code>
                <p className="key-warning">{t('export.keyWarning')}</p>
              </div>
            )}

            <div className="sign-actions">
              <button
                type="button"
                className="btn secondary"
                disabled={signing}
                onClick={() => handleSign(true)}
              >
                <Key size={14} />
                <span>
                  {validation?.signed ? t('export.reSignNewPair') : t('export.signNewPair')}
                </span>
              </button>

              <button
                type="button"
                className="btn text"
                onClick={() => setShowKeyInput(!showKeyInput)}
              >
                {showKeyInput ? t('export.hideKeyInput') : t('export.showKeyInput')}
              </button>
            </div>

            {showKeyInput && (
              <div className="custom-key-form">
                <label htmlFor="privkey-input">{t('export.privateKeyLabel')}</label>
                <input
                  id="privkey-input"
                  type="password"
                  placeholder={t('export.privateKeyPlaceholder')}
                  value={customKey}
                  onChange={(e) => setCustomKey(e.target.value)}
                  spellCheck={false}
                />
                <button
                  type="button"
                  className="btn primary"
                  disabled={signing || !customKey.trim()}
                  onClick={() => handleSign(false)}
                >
                  <span>{t('export.signWithKey')}</span>
                </button>
              </div>
            )}
          </section>

          {/* 3. ZIP パッケージのダウンロード */}
          <section className="export-section">
            <div className="section-title">
              <h3>{t('export.zipExportTitle')}</h3>
            </div>
            <p className="section-desc">{t('export.zipExportDesc')}</p>

            <div className="export-action-box">
              <a
                href={api.packageExportUrl()}
                download={filename}
                className="btn primary download-btn"
                role="button"
              >
                <Download size={16} />
                <span>{t('export.downloadZip', { filename })}</span>
              </a>
            </div>
          </section>
        </div>

        <div className="modal-footer">
          <button type="button" className="btn secondary" onClick={onClose}>
            {t('common.close')}
          </button>
        </div>
      </div>
    </div>
  );
}
