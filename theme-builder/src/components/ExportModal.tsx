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
import { api } from '../api.ts';
import type { PackageValidationResult } from '../types.ts';

export interface ExportModalProps {
  isOpen: boolean;
  hasDirtyFiles: boolean;
  onClose: () => void;
}

export function ExportModal({ isOpen, hasDirtyFiles, onClose }: ExportModalProps) {
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
        aria-label="ZIP パッケージ書き出しと署名"
      >
        <div className="modal-header">
          <div className="modal-title">
            <Package size={18} />
            <h2>ZIP パッケージ書き出しと署名</h2>
          </div>
          <button type="button" className="icon-btn" onClick={onClose} aria-label="閉じる">
            <X size={16} />
          </button>
        </div>

        <div className="modal-body">
          {hasDirtyFiles && (
            <div className="notice warning" role="alert">
              <AlertCircle size={15} />
              <span>未保存の変更があります。パッケージに反映するには保存してください。</span>
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
              <h3>パッケージ適合性検証</h3>
              {loading && <span className="loading-hint">検証中…</span>}
            </div>

            {validation && (
              <div className="validation-box">
                <ul className="check-list">
                  <li className={manifest ? 'pass' : 'fail'}>
                    {manifest ? <CheckCircle2 size={16} /> : <AlertCircle size={16} />}
                    <span>
                      theme.json:{' '}
                      {manifest ? `${manifest.name} (${manifest.id})` : '見つかりません'}
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
                    <span>templates/base.html (必須テンプレート)</span>
                  </li>
                  <li className={validation.signed ? 'pass' : 'info'}>
                    {validation.signed ? <ShieldCheck size={16} /> : <Key size={16} />}
                    <span>
                      デジタル署名 (signature.json):{' '}
                      {validation.signed ? '有効な署名あり' : '未署名（オプショナル）'}
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
              <h3>パッケージ署名 (Ed25519)</h3>
              {validation?.signed && <span className="badge success">署名済み</span>}
            </div>
            <p className="section-desc">
              テーマの全ファイル（signature.json を除く）のハッシュから Ed25519 署名を生成します。
            </p>

            {validation?.public_key && (
              <div className="pubkey-box">
                <span className="label">公開鍵 (Public Key):</span>
                <code className="pubkey">{validation.public_key}</code>
              </div>
            )}

            {generatedPrivKey && (
              <div className="generated-key-alert">
                <div className="alert-header">
                  <Key size={15} />
                  <strong>新規生成された秘密鍵 (Private Key)</strong>
                  <button
                    type="button"
                    className="copy-btn"
                    onClick={handleCopyPrivKey}
                    title="秘密鍵をコピー"
                  >
                    {copiedKey ? <Check size={13} /> : <Copy size={13} />}
                    <span>{copiedKey ? 'コピー完了' : 'コピー'}</span>
                  </button>
                </div>
                <code className="privkey">{generatedPrivKey}</code>
                <p className="key-warning">
                  ⚠️
                  この秘密鍵は再表示されません。テーマの更新・再配布時の署名用として安全に保管してください。
                </p>
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
                <span>{validation?.signed ? '新しい鍵ペアで再署名' : '鍵ペアを生成して署名'}</span>
              </button>

              <button
                type="button"
                className="btn text"
                onClick={() => setShowKeyInput(!showKeyInput)}
              >
                {showKeyInput ? '秘密鍵入力を隠す' : '既存の秘密鍵を指定して署名…'}
              </button>
            </div>

            {showKeyInput && (
              <div className="custom-key-form">
                <label htmlFor="privkey-input">
                  Ed25519 秘密鍵 (HEX 32バイト seed または PKCS#8):
                </label>
                <input
                  id="privkey-input"
                  type="password"
                  placeholder="64 文字の HEX 文字列"
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
                  <span>指定した鍵で署名</span>
                </button>
              </div>
            )}
          </section>

          {/* 3. ZIP パッケージのダウンロード */}
          <section className="export-section">
            <div className="section-title">
              <h3>ZIP アーカイブの書き出し</h3>
            </div>
            <p className="section-desc">
              テーマ構成ファイル（theme.json、templates/、assets/、signature.json 等）をまとめた ZIP
              を生成します。
            </p>

            <div className="export-action-box">
              <a
                href={api.packageExportUrl()}
                download={filename}
                className="btn primary download-btn"
                role="button"
              >
                <Download size={16} />
                <span>ZIP パッケージをダウンロード ({filename})</span>
              </a>
            </div>
          </section>
        </div>

        <div className="modal-footer">
          <button type="button" className="btn secondary" onClick={onClose}>
            閉じる
          </button>
        </div>
      </div>
    </div>
  );
}
