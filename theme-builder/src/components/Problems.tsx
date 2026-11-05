import { CircleAlert, CircleCheck, ListChecks } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { describeDiagnostic } from '../previewDoc.ts';
import type { CheckResult, Diagnostic } from '../types.ts';

export interface ProblemsProps {
  /** The error of the page being previewed (with the unsaved edits) */
  current: Diagnostic | null;
  sampleErrors: string[];
  /** The result of "check all samples" (saved files only) */
  check: CheckResult | null;
  checking: boolean;
  onCheck: () => void;
  onJump: (diagnostic: Diagnostic) => void;
}

export function Problems({
  current,
  sampleErrors,
  check,
  checking,
  onCheck,
  onJump,
}: ProblemsProps) {
  const { t } = useTranslation();
  const failures = check?.results.filter((r) => !r.ok) ?? [];
  const clean = !current && sampleErrors.length === 0;

  return (
    <section className="problems" aria-label={t('problems.title')}>
      <div className="problems-head">
        <h2>
          {clean ? (
            <CircleCheck size={14} className="ok" />
          ) : (
            <CircleAlert size={14} className="bad" />
          )}
          {t('problems.title')}
        </h2>
        <button type="button" onClick={onCheck} disabled={checking}>
          <ListChecks size={14} /> {checking ? t('problems.checking') : t('problems.checkAll')}
        </button>
      </div>
      <ul>
        {current && (
          <li>
            <button type="button" className="problem" onClick={() => onJump(current)}>
              {describeDiagnostic(current)}
            </button>
          </li>
        )}
        {sampleErrors.map((e) => (
          <li key={e} className="problem-text">
            {e}
          </li>
        ))}
        {failures.map((r) => (
          <li key={r.name}>
            <button
              type="button"
              className="problem"
              onClick={() => r.error && onJump(r.error)}
              title={t('problems.sampleTitle', { name: r.name })}
            >
              [{r.name}] {r.error ? describeDiagnostic(r.error) : r.template}
            </button>
          </li>
        ))}
        {clean && !check && <li className="muted">{t('problems.noProblems')}</li>}
        {check && failures.length === 0 && check.ok && (
          <li className="muted">{t('problems.allPassed', { count: check.results.length })}</li>
        )}
      </ul>
    </section>
  );
}
