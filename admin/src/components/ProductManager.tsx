import React, { useEffect, useRef, useState } from 'react';
import { ExternalLink, Images, Package, Pencil, Plus, RefreshCw, Trash2 } from 'lucide-react';
import { aegisApi, SERVER_URL, serverAssetUrl } from '../api/client';
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
        if (!cancelled) setError(messageOf(err, 'Failed to load products'));
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
    onShowToast(`${created ? 'Created' : 'Saved'} "${saved.title}"`);
  };

  const handleDelete = async (product: ContentItem) => {
    if (!window.confirm(`Delete "${product.title}"? Past orders keep their copy of it.`)) return;
    try {
      await aegisApi.deleteContent(productType, product.id);
      setProducts((prev) => prev?.filter((p) => p.id !== product.id) ?? null);
      setEditing(undefined);
      onShowToast(`Deleted "${product.title}"`);
    } catch (err) {
      onShowToast(messageOf(err, 'Failed to delete the product'), 'error');
    }
  };

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.heading}>
          <div className={styles.title}>
            <Package size={22} />
            <h1>Products</h1>
            {products && <span className={styles.count}>{products.length}</span>}
          </div>
          <div className={styles.headingActions}>
            <a
              className={styles.badge}
              href={`${SERVER_URL}/products`}
              target="_blank"
              rel="noreferrer"
              title="Open the storefront"
            >
              <ExternalLink size={12} /> Storefront
            </a>
            <Button variant="tool" onClick={() => setReloadCount((n) => n + 1)}>
              <RefreshCw size={14} />
              <span>Reload</span>
            </Button>
            <Button variant="primary" onClick={() => setEditing(null)} disabled={!config}>
              <Plus size={16} />
              <span>New product</span>
            </Button>
          </div>
        </div>

        {config && !config.enabled && (
          <div className={styles.notice}>
            The shop is not enabled, so products are listed on the site but cannot be ordered. Set{' '}
            <code>[commerce].enabled = true</code> to take orders (products with a buy URL link to
            it instead).
          </div>
        )}
        {error && <div className={cx(styles.notice, styles.error)}>{error}</div>}

        {products && (
          <div className={styles.tableWrap}>
            <table className={styles.table}>
              <thead>
                <tr>
                  <th>Product</th>
                  <th>SKU</th>
                  <th className={styles.numeric}>Price</th>
                  <th className={styles.numeric}>Stock</th>
                  <th>Status</th>
                  <th aria-label="Actions" />
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
                          <span className={styles.muted}>Unlimited</span>
                        ) : (
                          <span
                            className={cx(
                              styles.badge,
                              stock <= 0 ? styles.bad : stock <= 5 ? styles.warn : styles.good
                            )}
                          >
                            {stock <= 0 ? 'Sold out' : stock}
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
                          {product.status}
                        </span>
                      </td>
                      <td>
                        <div className={styles.rowActions}>
                          <Button
                            variant="icon"
                            aria-label={`Edit ${product.title}`}
                            title="Edit"
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
                            aria-label={`Delete ${product.title}`}
                            title="Delete"
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
                      No products yet. Create the first one.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        )}
        <p className={styles.hint}>
          Prices are in the smallest unit of {currency.toUpperCase()} (yen for JPY, cents for USD).
          Stock goes down when an order is placed and comes back when an unpaid order is cancelled
          or expires.
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
      setError('Title and slug are required.');
      return;
    }
    if (priceValue === null || priceValue === undefined) {
      setError('Enter the price as a whole number of the smallest currency unit.');
      return;
    }
    if (compareValue === undefined || stockValue === undefined) {
      setError('Compare-at price and stock must be whole numbers (or empty).');
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
        setError(messageOf(err, 'Failed to read the current stock'));
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
      setError(messageOf(err, 'Failed to save the product'));
      setIsSaving(false);
    }
  };

  return (
    <>
      <Modal
        title={product ? `Edit ${product.title}` : 'New product'}
        icon={<Package size={18} />}
        onClose={onClose}
        wide
      >
        <form className={styles.form} onSubmit={handleSubmit}>
          <FieldRow>
            <Field label="title:">
              <input
                ref={titleRef}
                value={title}
                onChange={(e) => {
                  setTitle(e.target.value);
                  if (!slugEdited) setSlug(toSlug(e.target.value));
                }}
                placeholder="Ceramic mug"
                aria-label="Title"
              />
            </Field>
            <Field label="slug:">
              <input
                value={slug}
                onChange={(e) => {
                  setSlug(e.target.value);
                  setSlugEdited(true);
                }}
                placeholder="ceramic-mug"
                aria-label="Slug"
              />
            </Field>
            <Field label="status:">
              <select
                value={status}
                onChange={(e) => setStatus(e.target.value)}
                aria-label="Status"
              >
                <option value="draft">Draft</option>
                <option value="published">Published</option>
              </select>
            </Field>
          </FieldRow>

          <div className={styles.formSection}>Price & stock ({currency.toUpperCase()})</div>
          <FieldRow>
            <Field label="price:">
              <input
                inputMode="numeric"
                value={price}
                onChange={(e) => setPrice(e.target.value)}
                placeholder="1200"
                aria-label="Price"
              />
            </Field>
            <Field label="compare at:">
              <input
                inputMode="numeric"
                value={compareAt}
                onChange={(e) => setCompareAt(e.target.value)}
                placeholder="Optional"
                aria-label="Compare-at price"
              />
            </Field>
            <Field label="stock:">
              <input
                inputMode="numeric"
                value={stock}
                onChange={(e) => setStock(e.target.value)}
                placeholder="Unlimited"
                aria-label="Stock"
              />
            </Field>
            <Field label="SKU:">
              <input
                value={sku}
                onChange={(e) => setSku(e.target.value)}
                placeholder="Optional"
                aria-label="SKU"
              />
            </Field>
          </FieldRow>
          <span className={styles.hint}>
            {price && !Number.isNaN(Number(price))
              ? `Shown as ${formatMoney(Number(price), currency)}. `
              : ''}
            Leave stock empty to sell without counting stock.
          </span>

          <div className={styles.formSection}>Image</div>
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
                  <span>Choose from library</span>
                </Button>
                {imageUrl && (
                  <Button variant="tool" onClick={() => setImageUrl('')}>
                    <span>Remove image</span>
                  </Button>
                )}
              </div>
            </div>
          </div>

          <div className={styles.formSection}>Description</div>
          <Field label="summary:">
            <input
              value={excerpt}
              onChange={(e) => setExcerpt(e.target.value)}
              placeholder="One line shown in product lists"
              maxLength={500}
              aria-label="Summary"
            />
          </Field>
          <textarea
            className={styles.textarea}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder={'Markdown: ## Details, - bullet points, **bold**…'}
            aria-label="Description (Markdown)"
          />
          <FieldRow>
            <Field label="category:">
              <select
                value={categoryId}
                onChange={(e) => setCategoryId(e.target.value)}
                aria-label="Category"
              >
                <option value="">No category</option>
                {categories.map((category) => (
                  <option key={category.id} value={category.id}>
                    {category.title}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="buy URL:">
              <input
                value={buyUrl}
                onChange={(e) => setBuyUrl(e.target.value)}
                placeholder="External checkout link (only when the shop is disabled)"
                aria-label="Buy URL"
              />
            </Field>
          </FieldRow>

          {error && <span className={styles.formError}>{error}</span>}
          <div className={styles.formActions}>
            <div>
              {product && (
                <Button danger onClick={() => onDelete(product)}>
                  <Trash2 size={14} />
                  <span>Delete</span>
                </Button>
              )}
            </div>
            <div>
              <Button onClick={onClose}>Cancel</Button>
              <Button type="submit" variant="primary" disabled={isSaving}>
                <span>{isSaving ? 'Saving...' : product ? 'Save product' : 'Create product'}</span>
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
