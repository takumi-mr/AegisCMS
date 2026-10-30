import React, { useEffect, useRef, useState } from 'react';
import {
  ChevronLeft,
  ChevronRight,
  ExternalLink,
  Images,
  MessageSquare,
  MessageSquareOff,
  Package,
  Pencil,
  Plus,
  RefreshCw,
  Search,
  Trash2,
  X,
} from 'lucide-react';
import { Trans, useTranslation } from 'react-i18next';
import { aegisApi, SERVER_URL, serverAssetUrl } from '../api/client';
import i18n from '../i18n';
import type {
  ContentItem,
  ProductVariant,
  ShopConfig,
  StorageMeta,
  UpsertVariantPayload,
  ContentSummary,
} from '../types';
import { isLabeledStatus } from '../utils/contentStatus';
import { STATUS_FILTERS } from '../utils/contentFilters';
import { cx } from '../utils/cx';
import { toDateTimeLocal } from '../utils/format';
import { blocksToMarkdown, markdownToBlocks } from '../utils/markdown';
import { formatMoney } from '../utils/money';
import { ImageUploader } from './ImageUploader';
import { MediaLibrary } from './MediaLibrary';
import { Button } from './ui/Button';
import { Field, FieldRow } from './ui/Field';
import { Modal } from './ui/Modal';
import styles from './Shop.module.css';

/** A product field as a number (the API may return numbers or numeric strings) */
const numberField = (item: ContentSummary, name: string): number | null => {
  const value = item.fields?.[name];
  if (typeof value === 'number') return value;
  if (typeof value === 'string' && value.trim() !== '' && !Number.isNaN(Number(value)))
    return Number(value);
  return null;
};

const textField = (item: ContentSummary, name: string): string => {
  const value = item.fields?.[name];
  return typeof value === 'string' ? value : '';
};

const toSlug = (value: string): string =>
  value
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, '-')
    .replace(/^-|-$/g, '');

const messageOf = (err: unknown, fallback: string): string =>
  err instanceof Error ? err.message : fallback;

interface ProductManagerProps {
  categories: ContentSummary[];
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/** Products per page of the list */
const PAGE_SIZE = 25;
/** Wait this long after the last key press before searching */
const SEARCH_DELAY_MS = 300;

/** Product catalog for editors: list, create, edit and delete products (`product` contents) */
export const ProductManager: React.FC<ProductManagerProps> = ({ categories, onShowToast }) => {
  const { t } = useTranslation();
  const [config, setConfig] = useState<ShopConfig | null>(null);
  const [products, setProducts] = useState<ContentSummary[] | null>(null);
  const [total, setTotal] = useState<number>(0);
  // The request the list on screen answers; while it differs from the wanted one, a load is running
  const [loadedKey, setLoadedKey] = useState<string>('');
  const [error, setError] = useState<string | null>(null);
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [page, setPage] = useState<number>(1);
  // The search box keeps what is typed; the list is searched once typing pauses
  const [draft, setDraft] = useState<string>('');
  const [query, setQuery] = useState<string>('');
  const [statusFilter, setStatusFilter] = useState<string>('');
  // undefined = closed, null = new product
  const [editing, setEditing] = useState<ContentItem | null | undefined>(undefined);
  const [openingId, setOpeningId] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getShopConfig()
      .then((loaded) => {
        if (!cancelled) setConfig(loaded);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(messageOf(err, i18n.t('products.loadFailed')));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (draft.trim() === query) return;
    const timer = setTimeout(() => {
      setQuery(draft.trim());
      setPage(1);
    }, SEARCH_DELAY_MS);
    return () => clearTimeout(timer);
  }, [draft, query]);

  const productType = config?.product_type;
  const listKey = JSON.stringify([query, statusFilter, page, reloadCount]);
  const loading = Boolean(productType) && loadedKey !== listKey;
  useEffect(() => {
    if (!productType) return;
    let cancelled = false;
    aegisApi
      .listContentSummaries(productType, {
        q: query,
        status: statusFilter,
        limit: PAGE_SIZE,
        offset: (page - 1) * PAGE_SIZE,
      })
      .then((result) => {
        if (cancelled) return;
        const lastPage = Math.max(1, Math.ceil(result.total / PAGE_SIZE));
        if (page > lastPage) {
          // The page is gone (its last item was deleted): show the last page instead
          setPage(lastPage);
          return;
        }
        setProducts(result.items);
        setTotal(result.total);
        setError(null);
        setLoadedKey(listKey);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setError(messageOf(err, i18n.t('products.loadFailed')));
        setLoadedKey(listKey);
      });
    return () => {
      cancelled = true;
    };
  }, [productType, listKey, query, statusFilter, page, reloadCount]);

  const currency = config?.currency ?? 'jpy';
  const isFiltered = query !== '' || statusFilter !== '';
  const lastPage = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const from = products && products.length > 0 ? (page - 1) * PAGE_SIZE + 1 : 0;
  const to = products && products.length > 0 ? from + products.length - 1 : 0;

  /** The list holds summaries only: the editor needs the blocks, so it loads the one product */
  const handleOpen = async (summary: ContentSummary) => {
    if (!productType || openingId) return;
    setOpeningId(summary.id);
    try {
      setEditing(await aegisApi.getContent(productType, summary.id));
    } catch (err) {
      onShowToast(messageOf(err, t('products.openFailed')), 'error');
    } finally {
      setOpeningId(null);
    }
  };

  const handleSaved = (saved: ContentItem, created: boolean) => {
    setEditing(undefined);
    if (created) setPage(1);
    setReloadCount((n) => n + 1);
    onShowToast(
      created
        ? t('products.created', { title: saved.title })
        : t('products.saved', { title: saved.title })
    );
  };

  const handleDelete = async (product: { id: string; title: string }) => {
    if (!productType) return;
    if (!window.confirm(t('products.confirmDelete', { title: product.title }))) return;
    try {
      await aegisApi.deleteContent(productType, product.id);
      setEditing(undefined);
      setReloadCount((n) => n + 1);
      onShowToast(t('products.deleted', { title: product.title }));
    } catch (err) {
      onShowToast(messageOf(err, t('products.deleteFailed')), 'error');
    }
  };

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.heading}>
          <div className={styles.title}>
            <Package size={22} />
            <h1>{t('products.title')}</h1>
            {products && <span className={styles.count}>{total}</span>}
          </div>
          <div className={styles.headingActions}>
            <a
              className={styles.badge}
              href={`${SERVER_URL}/products`}
              target="_blank"
              rel="noreferrer"
              title={t('products.storefrontTitle')}
            >
              <ExternalLink size={12} /> {t('products.storefront')}
            </a>
            <Button variant="tool" onClick={() => setReloadCount((n) => n + 1)}>
              <RefreshCw size={14} />
              <span>{t('common.reload')}</span>
            </Button>
            <Button variant="primary" onClick={() => setEditing(null)} disabled={!config}>
              <Plus size={16} />
              <span>{t('products.newProduct')}</span>
            </Button>
          </div>
        </div>

        {config && !config.enabled && (
          <div className={styles.notice}>
            <Trans i18nKey="products.shopDisabled" components={{ code: <code /> }} />
          </div>
        )}
        {error && <div className={cx(styles.notice, styles.error)}>{error}</div>}

        <div className={styles.toolbar}>
          <div className={styles.searchBox} role="search">
            <Search size={14} className={styles.searchIcon} aria-hidden="true" />
            <input
              type="search"
              value={draft}
              maxLength={100}
              placeholder={t('products.search')}
              aria-label={t('products.searchAria')}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  setQuery(draft.trim());
                  setPage(1);
                }
              }}
            />
            {draft !== '' && (
              <button
                type="button"
                className={styles.clearSearch}
                aria-label={t('products.clearSearch')}
                onClick={() => {
                  setDraft('');
                  setQuery('');
                  setPage(1);
                }}
              >
                <X size={13} />
              </button>
            )}
          </div>
          <select
            className={styles.statusFilter}
            value={statusFilter}
            aria-label={t('products.statusAria')}
            onChange={(e) => {
              setStatusFilter(e.target.value);
              setPage(1);
            }}
          >
            <option value="">{t('products.allStatuses')}</option>
            {STATUS_FILTERS.map((status) => (
              <option key={status} value={status}>
                {t(`common.status.${status}`)}
              </option>
            ))}
          </select>
        </div>

        {products && (
          <div className={cx(styles.tableWrap, loading && styles.loading)} aria-busy={loading}>
            <table className={styles.table}>
              <thead>
                <tr>
                  <th>{t('products.columns.product')}</th>
                  <th>{t('products.columns.sku')}</th>
                  <th className={styles.numeric}>{t('products.columns.price')}</th>
                  <th className={styles.numeric}>{t('products.columns.stock')}</th>
                  <th>{t('products.columns.status')}</th>
                  <th aria-label={t('products.columns.actions')} />
                </tr>
              </thead>
              <tbody>
                {products.map((product) => {
                  const price = numberField(product, 'price');
                  const stock = numberField(product, 'stock');
                  const image = textField(product, 'image_url');
                  return (
                    <tr
                      key={product.id}
                      className={styles.clickable}
                      onClick={() => void handleOpen(product)}
                    >
                      <td>
                        <div className={styles.productCell}>
                          {image ? (
                            <img className={styles.thumb} src={serverAssetUrl(image)} alt="" />
                          ) : (
                            <div className={styles.thumb} />
                          )}
                          <div className={styles.productNames}>
                            <strong>{product.title}</strong>
                            <span className={cx(styles.muted, styles.mono)}>/{product.slug}</span>
                          </div>
                        </div>
                      </td>
                      <td className={cx(styles.mono, styles.muted)}>
                        {textField(product, 'sku') || '—'}
                      </td>
                      <td className={styles.numeric}>
                        {price === null ? '—' : formatMoney(price, currency)}
                      </td>
                      <td className={styles.numeric}>
                        {stock === null ? (
                          <span className={styles.muted}>{t('products.unlimited')}</span>
                        ) : (
                          <span
                            className={cx(
                              styles.badge,
                              stock <= 0 ? styles.bad : stock <= 5 ? styles.warn : styles.good
                            )}
                          >
                            {stock <= 0 ? t('products.soldOut') : stock}
                          </span>
                        )}
                      </td>
                      <td>
                        <span
                          className={cx(
                            styles.badge,
                            product.status === 'published' && styles.good
                          )}
                        >
                          {isLabeledStatus(product.status)
                            ? t(`common.status.${product.status}`)
                            : product.status}
                        </span>
                      </td>
                      <td>
                        <div className={styles.rowActions}>
                          <Button
                            variant="icon"
                            aria-label={t('products.editAria', { title: product.title })}
                            title={t('common.edit')}
                            onClick={(e) => {
                              e.stopPropagation();
                              void handleOpen(product);
                            }}
                          >
                            <Pencil size={14} />
                          </Button>
                          <Button
                            variant="icon"
                            danger
                            aria-label={t('products.deleteAria', { title: product.title })}
                            title={t('common.delete')}
                            onClick={(e) => {
                              e.stopPropagation();
                              handleDelete(product);
                            }}
                          >
                            <Trash2 size={14} />
                          </Button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
                {products.length === 0 && (
                  <tr>
                    <td colSpan={6} className={styles.empty}>
                      {isFiltered ? t('products.noMatch') : t('products.empty')}
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        )}
        {total > PAGE_SIZE && (
          <nav className={styles.pagination} aria-label={t('products.pagination')}>
            <Button
              variant="icon"
              aria-label={t('products.previousPage')}
              disabled={page <= 1}
              onClick={() => setPage(page - 1)}
            >
              <ChevronLeft size={16} />
            </Button>
            <span aria-live="polite">{t('products.range', { from, to, total })}</span>
            <Button
              variant="icon"
              aria-label={t('products.nextPage')}
              disabled={page >= lastPage}
              onClick={() => setPage(page + 1)}
            >
              <ChevronRight size={16} />
            </Button>
          </nav>
        )}
        <p className={styles.hint}>
          {t('products.priceHint', { currency: currency.toUpperCase() })}
        </p>
      </div>

      {editing !== undefined && (
        <ProductEditor
          product={editing}
          productType={productType ?? 'product'}
          currency={currency}
          categories={categories}
          onSaved={handleSaved}
          onCommentsOpenChanged={(id, open) => {
            setEditing((prev) =>
              prev && prev.id === id ? { ...prev, comments_open: open } : prev
            );
          }}
          onShowToast={onShowToast}
          onDelete={handleDelete}
          onClose={() => setEditing(undefined)}
        />
      )}
    </main>
  );
};

interface VariantEditorModalProps {
  variant: ProductVariant | null;
  currency: string;
  onSave: (payload: UpsertVariantPayload) => Promise<void>;
  onClose: () => void;
}

const VariantEditorModal: React.FC<VariantEditorModalProps> = ({
  variant,
  currency,
  onSave,
  onClose,
}) => {
  const { t } = useTranslation();
  const [title, setTitle] = useState(variant?.title ?? '');
  const [sku, setSku] = useState(variant?.sku ?? '');
  const [priceOverride, setPriceOverride] = useState(
    variant?.price_override !== null && variant?.price_override !== undefined
      ? String(variant.price_override)
      : ''
  );
  const [stock, setStock] = useState(
    variant?.stock !== null && variant?.stock !== undefined ? String(variant.stock) : ''
  );
  const [attributesStr, setAttributesStr] = useState(() => {
    if (!variant?.attributes) return '';
    return Object.entries(variant.attributes)
      .map(([k, v]) => `${k}:${v}`)
      .join(', ');
  });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim()) {
      setError(t('products.errors.required'));
      return;
    }
    const attributes: Record<string, string> = {};
    if (attributesStr.trim()) {
      for (const part of attributesStr.split(',')) {
        const [k, v] = part.split(':').map((s) => s.trim());
        if (k && v) {
          attributes[k] = v;
        }
      }
    }
    const priceVal = priceOverride.trim() ? Number(priceOverride) : null;
    const stockVal = stock.trim() ? Number(stock) : null;

    setSaving(true);
    try {
      await onSave({
        id: variant?.id,
        title: title.trim(),
        sku: sku.trim() || null,
        price_override: priceVal,
        stock: stockVal,
        attributes,
      });
      onClose();
    } catch (err) {
      setError(messageOf(err, t('products.errors.saveFailed')));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title={variant ? t('products.variants.edit') : t('products.variants.add')}
      icon={<Package size={18} />}
      onClose={onClose}
      flush
    >
      <form onSubmit={handleSubmit} className={styles.form}>
        <Field label={t('products.variants.name')}>
          <input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Red / Small"
            required
            autoFocus
          />
        </Field>
        <FieldRow>
          <Field label={t('products.fields.sku')}>
            <input value={sku} onChange={(e) => setSku(e.target.value)} placeholder="TSH-RED-S" />
          </Field>
          <Field label={t('products.variants.priceOverride')}>
            <input
              inputMode="numeric"
              value={priceOverride}
              onChange={(e) => setPriceOverride(e.target.value)}
              placeholder={`${t('products.variants.inheritsBase')} (${currency.toUpperCase()})`}
            />
          </Field>
          <Field label={t('products.fields.stock')}>
            <input
              inputMode="numeric"
              value={stock}
              onChange={(e) => setStock(e.target.value)}
              placeholder={t('products.placeholders.unlimited')}
            />
          </Field>
        </FieldRow>
        <Field label={t('products.variants.attributesHint')}>
          <input
            value={attributesStr}
            onChange={(e) => setAttributesStr(e.target.value)}
            placeholder="color: Red, size: S"
          />
        </Field>

        {error && <span className={styles.formError}>{error}</span>}
        <div className={styles.formActions}>
          <div />
          <div>
            <Button onClick={onClose}>{t('common.cancel')}</Button>
            <Button type="submit" variant="primary" disabled={saving}>
              <span>{saving ? t('common.saving') : t('common.save')}</span>
            </Button>
          </div>
        </div>
      </form>
    </Modal>
  );
};

interface ProductEditorProps {
  /** null for a new product */
  product: ContentItem | null;
  productType: string;
  currency: string;
  categories: ContentSummary[];
  onSaved: (saved: ContentItem, created: boolean) => void;
  onCommentsOpenChanged: (id: string, open: boolean) => void;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
  onDelete: (product: ContentItem) => void;
  onClose: () => void;
}

const ProductEditor: React.FC<ProductEditorProps> = ({
  product,
  productType,
  currency,
  categories,
  onSaved,
  onCommentsOpenChanged,
  onShowToast,
  onDelete,
  onClose,
}) => {
  const { t } = useTranslation();
  const initialNumber = (name: string) => {
    const value = product ? numberField(product, name) : null;
    return value === null ? '' : String(value);
  };
  const [title, setTitle] = useState<string>(product?.title ?? '');
  const [slug, setSlug] = useState<string>(product?.slug ?? '');
  const [slugEdited, setSlugEdited] = useState<boolean>(Boolean(product));
  const [status, setStatus] = useState<string>(product?.status ?? 'draft');
  const [publishAt, setPublishAt] = useState<string>(
    product?.status === 'scheduled' && product.published_at
      ? toDateTimeLocal(product.published_at)
      : ''
  );
  const [unpublishAt, setUnpublishAt] = useState<string>(
    product?.unpublish_at ? toDateTimeLocal(product.unpublish_at) : ''
  );
  const [price, setPrice] = useState<string>(initialNumber('price'));
  const [compareAt, setCompareAt] = useState<string>(initialNumber('compare_at_price'));
  const [stock, setStock] = useState<string>(initialNumber('stock'));
  const [sku, setSku] = useState<string>(product ? textField(product, 'sku') : '');
  const [imageUrl, setImageUrl] = useState<string>(product ? textField(product, 'image_url') : '');
  const [excerpt, setExcerpt] = useState<string>(product ? textField(product, 'excerpt') : '');
  const [buyUrl, setBuyUrl] = useState<string>(product ? textField(product, 'buy_url') : '');
  const [categoryId, setCategoryId] = useState<string>(
    product ? textField(product, 'category_id') : ''
  );
  const [description, setDescription] = useState<string>(() =>
    product ? blocksToMarkdown(product.blocks ?? []) : ''
  );
  const [isLibraryOpen, setIsLibraryOpen] = useState<boolean>(false);
  const [isSaving, setIsSaving] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [variants, setVariants] = useState<ProductVariant[]>([]);
  const [loadingVariants, setLoadingVariants] = useState<boolean>(Boolean(product));
  const [editingVariant, setEditingVariant] = useState<ProductVariant | null | undefined>(
    undefined
  );
  const [commentsOpen, setCommentsOpen] = useState<boolean | undefined>(product?.comments_open);
  const [togglingComments, setTogglingComments] = useState<boolean>(false);

  const handleToggleComments = async () => {
    if (!product || commentsOpen === undefined || togglingComments) return;
    const nextOpen = !commentsOpen;
    setTogglingComments(true);
    try {
      const res = await aegisApi.setContentCommentsOpen(productType, product.id, nextOpen);
      setCommentsOpen(res.open);
      onCommentsOpenChanged(product.id, res.open);
      onShowToast(
        t(res.open ? 'comments.commentsOpenedToast' : 'comments.commentsClosedToast', {
          title: title.trim() || product.title,
        })
      );
    } catch (err) {
      onShowToast(messageOf(err, t('api.contentCommentsFailed')), 'error');
    } finally {
      setTogglingComments(false);
    }
  };

  useEffect(() => {
    if (!product) return;
    let cancelled = false;
    aegisApi
      .listVariants(productType, product.id)
      .then((items) => {
        if (!cancelled) {
          setVariants(items);
          setLoadingVariants(false);
        }
      })
      .catch(() => {
        if (!cancelled) setLoadingVariants(false);
      });
    return () => {
      cancelled = true;
    };
  }, [product, productType]);

  const handleSaveVariant = async (payload: UpsertVariantPayload) => {
    if (!product) return;
    const saved = await aegisApi.upsertVariant(productType, product.id, payload);
    setVariants((prev) => {
      const idx = prev.findIndex((v) => v.id === saved.id);
      if (idx >= 0) {
        const copy = [...prev];
        copy[idx] = saved;
        return copy;
      }
      return [...prev, saved];
    });
  };

  const titleRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    titleRef.current?.focus();
  }, []);

  const wholeNumber = (value: string): number | null | undefined => {
    if (value.trim() === '') return null;
    const n = Number(value);
    return Number.isInteger(n) && n >= 0 ? n : undefined;
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const priceValue = wholeNumber(price);
    const compareValue = wholeNumber(compareAt);
    const stockValue = wholeNumber(stock);
    if (!title.trim() || !slug.trim()) {
      setError(t('products.errors.required'));
      return;
    }
    if (priceValue === null || priceValue === undefined) {
      setError(t('products.errors.price'));
      return;
    }
    if (compareValue === undefined || stockValue === undefined) {
      setError(t('products.errors.numbers'));
      return;
    }
    const scheduledAt = status === 'scheduled' ? new Date(publishAt) : null;
    if (scheduledAt && Number.isNaN(scheduledAt.getTime())) {
      setError(t('products.errors.publishAt'));
      return;
    }
    const unpublishDate =
      (status === 'published' || status === 'scheduled') && unpublishAt
        ? new Date(unpublishAt)
        : null;
    if (unpublishDate && Number.isNaN(unpublishDate.getTime())) {
      setError(t('products.errors.unpublishAt'));
      return;
    }
    if (unpublishDate) {
      const compareDate = scheduledAt || new Date();
      if (unpublishDate.getTime() <= compareDate.getTime()) {
        setError(t('products.errors.unpublishBeforePublish'));
        return;
      }
    }
    // Orders change the stock while the form is open: unless it was edited here, save the
    // current stock instead of the value shown when the form opened (PUT replaces every field)
    let savedStock = stockValue;
    if (product && stock === initialNumber('stock')) {
      try {
        const latest = await aegisApi.getContent(productType, product.id);
        savedStock = numberField(latest, 'stock');
      } catch (err) {
        setError(messageOf(err, t('products.errors.stockFailed')));
        return;
      }
    }
    const payload: Record<string, unknown> = {
      title: title.trim(),
      slug: slug.trim(),
      status,
      ...(scheduledAt ? { published_at: scheduledAt.toISOString() } : {}),
      ...(unpublishDate ? { unpublish_at: unpublishDate.toISOString() } : { unpublish_at: null }),
      blocks: markdownToBlocks(description),
      price: priceValue,
      compare_at_price: compareValue,
      // Empty stock = not counted (unlimited)
      stock: savedStock,
      ...(sku.trim() ? { sku: sku.trim() } : {}),
      ...(imageUrl.trim() ? { image_url: imageUrl.trim() } : {}),
      ...(excerpt.trim() ? { excerpt: excerpt.trim() } : {}),
      ...(buyUrl.trim() ? { buy_url: buyUrl.trim() } : {}),
      ...(categoryId ? { category_id: categoryId } : {}),
    };
    setIsSaving(true);
    setError(null);
    try {
      const saved = product
        ? await aegisApi.updateContent(productType, product.id, payload)
        : await aegisApi.createContent(productType, payload as never);
      onSaved(saved, !product);
    } catch (err) {
      setError(messageOf(err, t('products.errors.saveFailed')));
      setIsSaving(false);
    }
  };

  return (
    <>
      <Modal
        title={
          product ? t('products.editTitle', { title: product.title }) : t('products.newProduct')
        }
        icon={<Package size={18} />}
        onClose={onClose}
        wide
        flush
      >
        <form className={styles.form} onSubmit={handleSubmit}>
          <FieldRow>
            <Field label={t('products.fields.title')}>
              <input
                ref={titleRef}
                value={title}
                onChange={(e) => {
                  setTitle(e.target.value);
                  if (!slugEdited) setSlug(toSlug(e.target.value));
                }}
                placeholder={t('products.placeholders.title')}
                aria-label={t('products.aria.title')}
              />
            </Field>
            <Field label={t('products.fields.slug')}>
              <input
                value={slug}
                onChange={(e) => {
                  setSlug(e.target.value);
                  setSlugEdited(true);
                }}
                placeholder={t('products.placeholders.slug')}
                aria-label={t('products.aria.slug')}
              />
            </Field>
            <Field label={t('products.fields.status')}>
              <select
                value={status}
                onChange={(e) => setStatus(e.target.value)}
                aria-label={t('products.aria.status')}
              >
                <option value="draft">{t('common.status.draft')}</option>
                <option value="scheduled">{t('common.status.scheduled')}</option>
                <option value="published">{t('common.status.published')}</option>
              </select>
            </Field>
            {status === 'scheduled' && (
              <Field label={t('products.fields.publishAt')}>
                <input
                  type="datetime-local"
                  value={publishAt}
                  onChange={(e) => setPublishAt(e.target.value)}
                  aria-label={t('products.aria.publishAt')}
                />
              </Field>
            )}
            {(status === 'published' || status === 'scheduled') && (
              <Field label={t('products.fields.unpublishAt')}>
                <input
                  type="datetime-local"
                  value={unpublishAt}
                  onChange={(e) => setUnpublishAt(e.target.value)}
                  aria-label={t('products.aria.unpublishAt')}
                />
              </Field>
            )}
          </FieldRow>

          <div className={styles.formSection}>
            {t('products.priceAndStock', { currency: currency.toUpperCase() })}
          </div>
          <FieldRow>
            <Field label={t('products.fields.price')}>
              <input
                inputMode="numeric"
                value={price}
                onChange={(e) => setPrice(e.target.value)}
                placeholder={t('products.placeholders.price')}
                aria-label={t('products.aria.price')}
              />
            </Field>
            <Field label={t('products.fields.compareAt')}>
              <input
                inputMode="numeric"
                value={compareAt}
                onChange={(e) => setCompareAt(e.target.value)}
                placeholder={t('products.placeholders.optional')}
                aria-label={t('products.aria.compareAt')}
              />
            </Field>
            <Field label={t('products.fields.stock')}>
              <input
                inputMode="numeric"
                value={stock}
                onChange={(e) => setStock(e.target.value)}
                placeholder={t('products.placeholders.unlimited')}
                aria-label={t('products.aria.stock')}
              />
            </Field>
            <Field label={t('products.fields.sku')}>
              <input
                value={sku}
                onChange={(e) => setSku(e.target.value)}
                placeholder={t('products.placeholders.optional')}
                aria-label={t('products.aria.sku')}
              />
            </Field>
          </FieldRow>
          <span className={styles.hint}>
            {price && !Number.isNaN(Number(price))
              ? t('products.shownAs', { price: formatMoney(Number(price), currency) })
              : ''}
            {t('products.stockHint')}
          </span>

          <div className={styles.formSection}>{t('products.image')}</div>
          <div className={styles.imageRow}>
            {imageUrl ? (
              <img className={styles.preview} src={serverAssetUrl(imageUrl)} alt="" />
            ) : (
              <div className={styles.preview} />
            )}
            <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 8 }}>
              <ImageUploader onUploaded={(meta: StorageMeta) => setImageUrl(meta.url)} />
              <div style={{ display: 'flex', gap: 8 }}>
                <Button variant="tool" onClick={() => setIsLibraryOpen(true)}>
                  <Images size={14} />
                  <span>{t('products.chooseFromLibrary')}</span>
                </Button>
                {imageUrl && (
                  <Button variant="tool" onClick={() => setImageUrl('')}>
                    <span>{t('products.removeImage')}</span>
                  </Button>
                )}
              </div>
            </div>
          </div>

          <div className={styles.formSection}>{t('products.description')}</div>
          <Field label={t('products.fields.summary')}>
            <input
              value={excerpt}
              onChange={(e) => setExcerpt(e.target.value)}
              placeholder={t('products.placeholders.summary')}
              maxLength={500}
              aria-label={t('products.aria.summary')}
            />
          </Field>
          <textarea
            className={styles.textarea}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder={t('products.placeholders.description')}
            aria-label={t('products.aria.description')}
          />
          <FieldRow>
            <Field label={t('products.fields.category')}>
              <select
                value={categoryId}
                onChange={(e) => setCategoryId(e.target.value)}
                aria-label={t('products.aria.category')}
              >
                <option value="">{t('products.noCategory')}</option>
                {categories.map((category) => (
                  <option key={category.id} value={category.id}>
                    {category.title}
                  </option>
                ))}
              </select>
            </Field>
            <Field label={t('products.fields.buyUrl')}>
              <input
                value={buyUrl}
                onChange={(e) => setBuyUrl(e.target.value)}
                placeholder={t('products.placeholders.buyUrl')}
                aria-label={t('products.aria.buyUrl')}
              />
            </Field>
            {product && commentsOpen !== undefined && (
              <Button
                type="button"
                variant="tool"
                disabled={togglingComments}
                onClick={handleToggleComments}
                title={t(commentsOpen ? 'comments.closeComments' : 'comments.openComments')}
                aria-label={t(commentsOpen ? 'comments.closeComments' : 'comments.openComments')}
              >
                {commentsOpen ? <MessageSquare size={14} /> : <MessageSquareOff size={14} />}
                <span>
                  {t(commentsOpen ? 'editor.commentsAccepting' : 'editor.commentsStopped')}
                </span>
              </Button>
            )}
          </FieldRow>

          <div
            className={styles.formSection}
            style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}
          >
            <span>{t('products.variants.title')}</span>
            {product && (
              <Button type="button" variant="tool" onClick={() => setEditingVariant(null)}>
                <Plus size={14} />
                <span>{t('products.variants.add')}</span>
              </Button>
            )}
          </div>
          {!product ? (
            <p className={styles.hint} style={{ margin: '8px 0 16px' }}>
              {t('products.variants.createFirstHint')}
            </p>
          ) : (
            <div style={{ marginBottom: 16 }}>
              {loadingVariants ? (
                <p className={styles.hint}>{t('common.loading')}</p>
              ) : variants.length === 0 ? (
                <p className={styles.hint}>{t('products.variants.empty')}</p>
              ) : (
                <div className={styles.tableWrap} style={{ margin: '8px 0' }}>
                  <table className={styles.table} style={{ width: '100%' }}>
                    <thead>
                      <tr>
                        <th>{t('products.variants.name')}</th>
                        <th>{t('products.fields.sku')}</th>
                        <th>{t('products.variants.priceOverride')}</th>
                        <th>{t('products.fields.stock')}</th>
                        <th style={{ width: 80 }}></th>
                      </tr>
                    </thead>
                    <tbody>
                      {variants.map((v) => (
                        <tr key={v.id}>
                          <td>
                            <strong>{v.title}</strong>
                          </td>
                          <td>{v.sku || '-'}</td>
                          <td>
                            {v.price_override !== null
                              ? formatMoney(v.price_override, currency)
                              : t('products.variants.inheritsBase')}
                          </td>
                          <td>
                            {v.stock !== null ? v.stock : t('products.placeholders.unlimited')}
                          </td>
                          <td>
                            <div className={styles.rowActions}>
                              <Button
                                type="button"
                                variant="icon"
                                title={t('common.edit')}
                                onClick={() => setEditingVariant(v)}
                              >
                                <Pencil size={14} />
                              </Button>
                              <Button
                                type="button"
                                variant="icon"
                                danger
                                title={t('common.delete')}
                                onClick={async () => {
                                  if (
                                    !window.confirm(
                                      t('products.variants.confirmDelete', { title: v.title })
                                    )
                                  )
                                    return;
                                  try {
                                    await aegisApi.deleteVariant(productType, product.id, v.id);
                                    setVariants((prev) => prev.filter((item) => item.id !== v.id));
                                  } catch (err) {
                                    alert(messageOf(err, t('products.variants.deleteFailed')));
                                  }
                                }}
                              >
                                <Trash2 size={14} />
                              </Button>
                            </div>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}

          {error && <span className={styles.formError}>{error}</span>}
          <div className={styles.formActions}>
            <div>
              {product && (
                <Button danger onClick={() => onDelete(product)}>
                  <Trash2 size={14} />
                  <span>{t('common.delete')}</span>
                </Button>
              )}
            </div>
            <div>
              <Button onClick={onClose}>{t('common.cancel')}</Button>
              <Button type="submit" variant="primary" disabled={isSaving}>
                <span>
                  {isSaving
                    ? t('common.saving')
                    : product
                      ? t('products.save')
                      : t('products.create')}
                </span>
              </Button>
            </div>
          </div>
        </form>
      </Modal>
      {isLibraryOpen && (
        <MediaLibrary
          onSelect={(object) => {
            setImageUrl(object.url);
            setIsLibraryOpen(false);
          }}
          onClose={() => setIsLibraryOpen(false)}
        />
      )}
      {editingVariant !== undefined && (
        <VariantEditorModal
          variant={editingVariant}
          currency={currency}
          onSave={handleSaveVariant}
          onClose={() => setEditingVariant(undefined)}
        />
      )}
    </>
  );
};
