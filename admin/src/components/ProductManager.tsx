import React, { useEffect, useRef, useState } from 'react';
import { ExternalLink, Images, Package, Pencil, Plus, RefreshCw, Trash2 } from 'lucide-react';
import { Trans, useTranslation } from 'react-i18next';
import { aegisApi, SERVER_URL, serverAssetUrl } from '../api/client';
import i18n from '../i18n';
import type { ContentItem, ShopConfig, StorageMeta } from '../types';
import { cx } from '../utils/cx';
import { blocksToMarkdown, markdownToBlocks } from '../utils/markdown';
import { formatMoney } from '../utils/money';
import { ImageUploader } from './ImageUploader';
import { MediaLibrary } from './MediaLibrary';
import { Button } from './ui/Button';
import { Field, FieldRow } from './ui/Field';
import { Modal } from './ui/Modal';
import styles from './Shop.module.css';

/** A product field as a number (the API may return numbers or numeric strings) */
const numberField = (item: ContentItem, name: string): number | null => {
  const value = item.fields?.[name];
  if (typeof value === 'number') return value;
  if (typeof value === 'string' && value.trim() !== '' && !Number.isNaN(Number(value)))
    return Number(value);
  return null;
};

const textField = (item: ContentItem, name: string): string => {
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
  categories: ContentItem[];
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/** Product catalog for editors: list, create, edit and delete products (`product` contents) */
export const ProductManager: React.FC<ProductManagerProps> = ({ categories, onShowToast }) => {
  const { t } = useTranslation();
  const [config, setConfig] = useState<ShopConfig | null>(null);
  const [products, setProducts] = useState<ContentItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reloadCount, setReloadCount] = useState<number>(0);
  // undefined = closed, null = new product
  const [editing, setEditing] = useState<ContentItem | null | undefined>(undefined);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getShopConfig()
      .then((loaded) => {
        if (cancelled) return null;
        setConfig(loaded);
        return aegisApi.getContents(loaded.product_type);
      })
      .then((items) => {
        if (cancelled || !items) return;
        setProducts(items);
        setError(null);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(messageOf(err, i18n.t('products.loadFailed')));
      });
    return () => {
      cancelled = true;
    };
  }, [reloadCount]);

  const currency = config?.currency ?? 'jpy';
  const productType = config?.product_type ?? 'product';

  const handleSaved = (saved: ContentItem, created: boolean) => {
    setProducts((prev) => {
      const rest = (prev ?? []).filter((p) => p.id !== saved.id);
      return created ? [saved, ...rest] : (prev ?? []).map((p) => (p.id === saved.id ? saved : p));
    });
    setEditing(undefined);
    onShowToast(
      created
        ? t('products.created', { title: saved.title })
        : t('products.saved', { title: saved.title })
    );
  };

  const handleDelete = async (product: ContentItem) => {
    if (!window.confirm(t('products.confirmDelete', { title: product.title }))) return;
    try {
      await aegisApi.deleteContent(productType, product.id);
      setProducts((prev) => prev?.filter((p) => p.id !== product.id) ?? null);
      setEditing(undefined);
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
            {products && <span className={styles.count}>{products.length}</span>}
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

        {products && (
          <div className={styles.tableWrap}>
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
                      onClick={() => setEditing(product)}
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
                          {product.status === 'published' || product.status === 'draft'
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
                              setEditing(product);
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
                      {t('products.empty')}
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        )}
        <p className={styles.hint}>
          {t('products.priceHint', { currency: currency.toUpperCase() })}
        </p>
      </div>

      {editing !== undefined && (
        <ProductEditor
          product={editing}
          productType={productType}
          currency={currency}
          categories={categories}
          onSaved={handleSaved}
          onDelete={handleDelete}
          onClose={() => setEditing(undefined)}
        />
      )}
    </main>
  );
};

interface ProductEditorProps {
  /** null for a new product */
  product: ContentItem | null;
  productType: string;
  currency: string;
  categories: ContentItem[];
  onSaved: (saved: ContentItem, created: boolean) => void;
  onDelete: (product: ContentItem) => void;
  onClose: () => void;
}

const ProductEditor: React.FC<ProductEditorProps> = ({
  product,
  productType,
  currency,
  categories,
  onSaved,
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
                <option value="published">{t('common.status.published')}</option>
              </select>
            </Field>
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
          </FieldRow>

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
    </>
  );
};
