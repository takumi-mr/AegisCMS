import React, { useEffect, useState } from 'react';
import { ChevronLeft, ChevronRight, Download, Mail, Search, Send, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, ApiError } from '../api/client';
import type {
  ContentSummary,
  NewsletterCounts,
  NewsletterIssue,
  NewsletterStatus,
  NewsletterSubscriber,
} from '../types';
import { cx } from '../utils/cx';
import { formatDate } from '../utils/format';
import { Button } from './ui/Button';
import { Field, FieldRow } from './ui/Field';
import styles from './NewsletterManager.module.css';

const PAGE_SIZE = 25;
/** Wait this long after the last key press before searching */
const SEARCH_DELAY_MS = 300;
/** Published articles offered in the send form (the newest ones) */
const CONTENT_CHOICES = 100;
const CONTENT_TYPE = 'article';

interface NewsletterManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

const messageOf = (err: unknown, fallback: string): string =>
  err instanceof Error ? err.message : fallback;

const EMPTY_COUNTS: NewsletterCounts = { pending: 0, active: 0, unsubscribed: 0 };

/** Newsletter subscribers, sending published articles to them, and what was sent */
export const NewsletterManager: React.FC<NewsletterManagerProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [counts, setCounts] = useState<NewsletterCounts>(EMPTY_COUNTS);
  const [subscribers, setSubscribers] = useState<NewsletterSubscriber[] | null>(null);
  const [total, setTotal] = useState<number>(0);
  const [page, setPage] = useState<number>(1);
  const [status, setStatus] = useState<NewsletterStatus | ''>('');
  const [draft, setDraft] = useState<string>('');
  const [query, setQuery] = useState<string>('');
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [error, setError] = useState<string | null>(null);
  const [issues, setIssues] = useState<NewsletterIssue[]>([]);
  const [contents, setContents] = useState<ContentSummary[]>([]);
  // The send form
  const [contentId, setContentId] = useState<string>('');
  const [summary, setSummary] = useState<string>('');
  const [testTo, setTestTo] = useState<string>('');
  const [sending, setSending] = useState<boolean>(false);

  useEffect(() => {
    if (draft.trim() === query) return;
    const timer = setTimeout(() => {
      setQuery(draft.trim());
      setPage(1);
    }, SEARCH_DELAY_MS);
    return () => clearTimeout(timer);
  }, [draft, query]);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listNewsletterSubscribers({
        status,
        q: query,
        limit: PAGE_SIZE,
        offset: (page - 1) * PAGE_SIZE,
      })
      .then((result) => {
        if (cancelled) return;
        const lastPage = Math.max(1, Math.ceil(result.total / PAGE_SIZE));
        if (page > lastPage) {
          // The page is gone (its last subscriber was deleted): show the last page instead
          setPage(lastPage);
          return;
        }
        setSubscribers(result.subscribers);
        setTotal(result.total);
        setCounts(result.counts);
        setError(null);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setError(
          err instanceof ApiError && err.code === 'newsletter_disabled'
            ? t('newsletter.disabled')
            : messageOf(err, t('newsletter.loadFailed'))
        );
      });
    return () => {
      cancelled = true;
    };
  }, [status, query, page, reloadCount, t]);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listNewsletterIssues({ limit: 10 })
      .then((result) => {
        if (!cancelled) setIssues(result.issues);
      })
      .catch(() => {
        if (!cancelled) setIssues([]);
      });
    return () => {
      cancelled = true;
    };
  }, [reloadCount]);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listContentSummaries(CONTENT_TYPE, { status: 'published', limit: CONTENT_CHOICES })
      .then((result) => {
        if (!cancelled) setContents(result.items);
      })
      .catch(() => {
        if (!cancelled) setContents([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const chosen = contents.find((c) => c.id === contentId);

  const send = async (test: boolean, force = false): Promise<void> => {
    if (!chosen) {
      onShowToast(t('newsletter.chooseFirst'), 'error');
      return;
    }
    if (test && !testTo.trim()) {
      onShowToast(t('newsletter.testEmailRequired'), 'error');
      return;
    }
    if (
      !test &&
      !force &&
      !window.confirm(t('newsletter.confirmSend', { title: chosen.title, count: counts.active }))
    ) {
      return;
    }
    setSending(true);
    try {
      const result = await aegisApi.sendNewsletterIssue({
        content_type: CONTENT_TYPE,
        slug_or_id: chosen.id,
        summary: summary.trim() || undefined,
        test_to: test ? testTo.trim() : undefined,
        force: force || undefined,
      });
      if (test) {
        onShowToast(t('newsletter.testSent', { email: testTo.trim() }));
      } else {
        onShowToast(t('newsletter.sent', { count: result.recipients ?? 0 }));
        setReloadCount((n) => n + 1);
      }
    } catch (err) {
      if (!test && !force && err instanceof ApiError && err.code === 'already_sent') {
        const previous = issues.find((i) => i.content_id === chosen.id);
        setSending(false);
        if (
          window.confirm(
            t('newsletter.alreadySent', {
              title: chosen.title,
              date: previous ? formatDate(previous.created_at) : '',
            })
          )
        ) {
          await send(false, true);
        }
        return;
      }
      onShowToast(messageOf(err, t('newsletter.sendFailed')), 'error');
    } finally {
      setSending(false);
    }
  };

  const handleDelete = async (subscriber: NewsletterSubscriber) => {
    if (!window.confirm(t('newsletter.confirmDelete', { email: subscriber.email }))) return;
    try {
      await aegisApi.deleteNewsletterSubscribers([subscriber.id]);
      setReloadCount((n) => n + 1);
      onShowToast(t('newsletter.deleted', { email: subscriber.email }));
    } catch (err) {
      onShowToast(messageOf(err, t('newsletter.deleteFailed')), 'error');
    }
  };

  const handleExport = async () => {
    try {
      const csv = await aegisApi.exportNewsletterSubscribers();
      const url = URL.createObjectURL(new Blob([csv], { type: 'text/csv;charset=utf-8' }));
      const link = document.createElement('a');
      link.href = url;
      link.download = 'newsletter-subscribers.csv';
      link.click();
      URL.revokeObjectURL(url);
      onShowToast(t('newsletter.exported'));
    } catch (err) {
      onShowToast(messageOf(err, t('newsletter.exportFailed')), 'error');
    }
  };

  const statusLabel = (value: NewsletterStatus): string =>
    value === 'active'
      ? t('newsletter.statusActive')
      : value === 'pending'
        ? t('newsletter.statusPending')
        : t('newsletter.statusUnsubscribed');

  const lastPage = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const first = subscribers && subscribers.length > 0 ? (page - 1) * PAGE_SIZE + 1 : 0;
  const last = subscribers && subscribers.length > 0 ? first + subscribers.length - 1 : 0;

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.title}>
          <Mail size={22} />
          <h1>{t('newsletter.title')}</h1>
        </div>
        <p className={styles.description}>{t('newsletter.description')}</p>
        {error && <div className={cx(styles.notice, styles.error)}>{error}</div>}

        <div className={styles.counts}>
          {(['active', 'pending', 'unsubscribed'] as const).map((value) => (
            <div key={value} className={styles.count}>
              <span className={styles.countNumber}>{counts[value]}</span>
              <span className={styles.countLabel}>{statusLabel(value)}</span>
            </div>
          ))}
        </div>

        <section className={styles.panel} aria-labelledby="newsletter-send-title">
          <h2 id="newsletter-send-title" className={styles.panelTitle}>
            {t('newsletter.sendTitle')}
          </h2>
          <p className={styles.hint}>{t('newsletter.sendHint')}</p>
          <Field label={t('newsletter.content')}>
            <select
              value={contentId}
              onChange={(e) => setContentId(e.target.value)}
              aria-label={t('newsletter.contentAria')}
            >
              <option value="">
                {contents.length === 0 ? t('newsletter.noContent') : t('newsletter.chooseContent')}
              </option>
              {contents.map((content) => (
                <option key={content.id} value={content.id}>
                  {content.title}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t('newsletter.summary')}>
            <textarea
              value={summary}
              onChange={(e) => setSummary(e.target.value)}
              placeholder={chosen?.excerpt || t('newsletter.summaryPlaceholder')}
              maxLength={500}
              rows={3}
              aria-label={t('newsletter.summaryAria')}
            />
          </Field>
          <FieldRow>
            <Field label={t('newsletter.testTo')}>
              <input
                type="email"
                value={testTo}
                onChange={(e) => setTestTo(e.target.value)}
                placeholder="you@example.com"
                aria-label={t('newsletter.testToAria')}
              />
            </Field>
          </FieldRow>
          <div className={styles.formActions}>
            <Button type="button" disabled={sending} onClick={() => void send(true)}>
              {t('newsletter.sendTest')}
            </Button>
            <Button
              type="button"
              variant="primary"
              disabled={sending || counts.active === 0}
              aria-label={t('newsletter.sendAria')}
              onClick={() => void send(false)}
            >
              <Send size={14} aria-hidden="true" />
              {t('newsletter.send', { count: counts.active })}
            </Button>
          </div>
        </section>

        <section className={styles.panel} aria-labelledby="newsletter-subscribers-title">
          <div className={styles.panelHeader}>
            <h2 id="newsletter-subscribers-title" className={styles.panelTitle}>
              {t('newsletter.subscribers')}
              {subscribers && <span className={styles.totalBadge}>{total}</span>}
            </h2>
            <Button
              type="button"
              title={t('newsletter.exportTitle')}
              disabled={counts.active === 0}
              onClick={() => void handleExport()}
            >
              <Download size={14} aria-hidden="true" />
              {t('newsletter.export')}
            </Button>
          </div>
          <div className={styles.filters}>
            <div className={styles.search} role="search">
              <Search size={14} className={styles.searchIcon} aria-hidden="true" />
              <input
                type="search"
                value={draft}
                maxLength={100}
                placeholder={t('newsletter.search')}
                aria-label={t('newsletter.searchAria')}
                onChange={(e) => setDraft(e.target.value)}
              />
            </div>
            <select
              className={styles.statusFilter}
              value={status}
              aria-label={t('newsletter.statusFilter')}
              onChange={(e) => {
                setStatus(e.target.value as NewsletterStatus | '');
                setPage(1);
              }}
            >
              <option value="">{t('newsletter.all')}</option>
              <option value="active">{t('newsletter.statusActive')}</option>
              <option value="pending">{t('newsletter.statusPending')}</option>
              <option value="unsubscribed">{t('newsletter.statusUnsubscribed')}</option>
            </select>
          </div>

          {subscribers && (
            <div className={styles.list}>
              {subscribers.map((subscriber) => (
                <div key={subscriber.id} className={styles.row}>
                  <div className={styles.rowMain}>
                    <div className={styles.rowEmail}>{subscriber.email}</div>
                    <div className={styles.rowMeta}>
                      {t('newsletter.since', { date: formatDate(subscriber.created_at) })}
                    </div>
                  </div>
                  <span className={cx(styles.badge, styles[`badge_${subscriber.status}`])}>
                    {statusLabel(subscriber.status)}
                  </span>
                  <Button
                    variant="icon"
                    danger
                    aria-label={t('newsletter.deleteAria', { email: subscriber.email })}
                    title={t('common.delete')}
                    onClick={() => void handleDelete(subscriber)}
                  >
                    <Trash2 size={14} />
                  </Button>
                </div>
              ))}
              {subscribers.length === 0 && (
                <div className={styles.empty}>
                  {query || status ? t('newsletter.noMatch') : t('newsletter.empty')}
                </div>
              )}
            </div>
          )}

          {total > PAGE_SIZE && (
            <nav className={styles.pagination} aria-label={t('newsletter.pagination')}>
              <Button
                variant="icon"
                aria-label={t('newsletter.previousPage')}
                disabled={page <= 1}
                onClick={() => setPage(page - 1)}
              >
                <ChevronLeft size={16} />
              </Button>
              <span aria-live="polite">
                {t('newsletter.range', { from: first, to: last, total })}
              </span>
              <Button
                variant="icon"
                aria-label={t('newsletter.nextPage')}
                disabled={page >= lastPage}
                onClick={() => setPage(page + 1)}
              >
                <ChevronRight size={16} />
              </Button>
            </nav>
          )}
        </section>

        <section className={styles.panel} aria-labelledby="newsletter-history-title">
          <h2 id="newsletter-history-title" className={styles.panelTitle}>
            {t('newsletter.history')}
          </h2>
          {issues.length === 0 ? (
            <div className={styles.empty}>{t('newsletter.historyEmpty')}</div>
          ) : (
            <div className={styles.list}>
              {issues.map((issue) => (
                <div key={issue.id} className={styles.row}>
                  <div className={styles.rowMain}>
                    <div className={styles.rowEmail}>{issue.title}</div>
                    <div className={styles.rowMeta}>{formatDate(issue.created_at)}</div>
                  </div>
                  <span className={styles.recipients}>
                    {t('newsletter.recipients', { count: issue.recipients })}
                  </span>
                </div>
              ))}
            </div>
          )}
        </section>
      </div>
    </main>
  );
};
