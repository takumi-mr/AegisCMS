import React, { useState } from 'react';
import { ArrowDown, ArrowUp, Images, Plus, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { BlockNode, PluginBlockDefinition, PluginBlockField } from '../types';
import { altFromKey } from '../utils/media';
import {
  MAX_LIST_ITEMS,
  defaultValues,
  listItems,
  localize,
  missingRequired,
  missingRequiredFields,
} from '../utils/pluginBlocks';
import { ImageUploader } from './ImageUploader';
import { MediaLibrary } from './MediaLibrary';
import { Button } from './ui/Button';
import styles from './PluginBlockEditor.module.css';

interface PluginBlockEditorProps {
  block: BlockNode;
  definition: PluginBlockDefinition;
  onChange: (updated: BlockNode) => void;
  /** Uploading needs the editor role; viewers can still pick from the media library */
  canUpload: boolean;
}

interface FieldControlProps {
  field: PluginBlockField;
  id: string;
  value: unknown;
  /** `undefined` removes the value */
  onChange: (value: unknown) => void;
  canUpload: boolean;
}

/** One input of a field, by its type. A `list` renders the form of each of its items */
const FieldControl: React.FC<FieldControlProps> = ({ field, id, value, onChange, canUpload }) => {
  const { t, i18n } = useTranslation();
  const [libraryOpen, setLibraryOpen] = useState(false);
  const placeholder = localize(field.placeholder, i18n.language);

  switch (field.type) {
    case 'list': {
      const subFields = field.fields ?? [];
      const max = field.max_items ?? MAX_LIST_ITEMS;
      const items = listItems(value);
      const setItems = (next: Record<string, unknown>[]) => onChange(next);
      const setItemValue = (index: number, name: string, itemValue: unknown) => {
        const item = { ...items[index] };
        if (itemValue === undefined) {
          delete item[name];
        } else {
          item[name] = itemValue;
        }
        setItems(items.map((existing, i) => (i === index ? item : existing)));
      };
      const move = (index: number, to: number) => {
        const next = [...items];
        [next[index], next[to]] = [next[to], next[index]];
        setItems(next);
      };
      return (
        <div className={styles.items} id={id}>
          {items.map((item, index) => {
            const itemMissing = missingRequired(subFields, item);
            return (
              <div key={index} className={styles.item}>
                <div className={styles.itemHeader}>
                  <span className={styles.itemTitle}>
                    {t('blocks.plugin.item', { n: index + 1 })}
                  </span>
                  <span className={styles.itemActions}>
                    <button
                      type="button"
                      className={styles.iconButton}
                      aria-label={t('blocks.plugin.moveUp', { n: index + 1 })}
                      title={t('blocks.plugin.moveUp', { n: index + 1 })}
                      disabled={index === 0}
                      onClick={() => move(index, index - 1)}
                    >
                      <ArrowUp size={14} />
                    </button>
                    <button
                      type="button"
                      className={styles.iconButton}
                      aria-label={t('blocks.plugin.moveDown', { n: index + 1 })}
                      title={t('blocks.plugin.moveDown', { n: index + 1 })}
                      disabled={index === items.length - 1}
                      onClick={() => move(index, index + 1)}
                    >
                      <ArrowDown size={14} />
                    </button>
                    <button
                      type="button"
                      className={styles.iconButton}
                      aria-label={t('blocks.plugin.removeItem', { n: index + 1 })}
                      title={t('blocks.plugin.removeItem', { n: index + 1 })}
                      onClick={() => setItems(items.filter((_, i) => i !== index))}
                    >
                      <Trash2 size={14} />
                    </button>
                  </span>
                </div>
                {subFields.map((sub) => (
                  <FieldRow
                    key={sub.name}
                    field={sub}
                    id={`${id}-${index}-${sub.name}`}
                    value={item[sub.name]}
                    missing={itemMissing.includes(sub.name)}
                    onChange={(v) => setItemValue(index, sub.name, v)}
                    canUpload={canUpload}
                  />
                ))}
              </div>
            );
          })}
          <div className={styles.itemsFooter}>
            <Button
              disabled={items.length >= max}
              onClick={() => setItems([...items, defaultValues(subFields)])}
            >
              <Plus size={14} />
              <span>{t('blocks.plugin.addItem')}</span>
            </Button>
            {field.max_items != null && (
              <span className={styles.help}>
                {t('blocks.plugin.itemCount', { count: items.length, max })}
              </span>
            )}
          </div>
        </div>
      );
    }
    case 'text':
      return (
        <textarea
          id={id}
          className={styles.textarea}
          placeholder={placeholder}
          value={typeof value === 'string' ? value : ''}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case 'number':
      return (
        <input
          id={id}
          type="number"
          className={styles.input}
          placeholder={placeholder}
          value={typeof value === 'number' ? value : ''}
          onChange={(e) => onChange(e.target.value === '' ? undefined : Number(e.target.value))}
        />
      );
    case 'boolean':
      return (
        <input
          id={id}
          type="checkbox"
          checked={value === true}
          onChange={(e) => onChange(e.target.checked)}
        />
      );
    case 'select':
      return (
        <select
          id={id}
          className={styles.input}
          value={typeof value === 'string' ? value : ''}
          onChange={(e) => onChange(e.target.value)}
        >
          {!field.required && <option value="">—</option>}
          {(field.options ?? []).map((option) => (
            <option key={option.value} value={option.value}>
              {localize(option.label, i18n.language)}
            </option>
          ))}
        </select>
      );
    case 'image':
      return (
        <div className={styles.image}>
          {canUpload && <ImageUploader onUploaded={(meta) => onChange(meta.url)} />}
          <Button className={styles.library} onClick={() => setLibraryOpen(true)}>
            <Images size={14} />
            <span>{t('blocks.library')}</span>
          </Button>
          <input
            id={id}
            type="text"
            className={styles.input}
            placeholder={placeholder || t('blocks.urlPlaceholder')}
            value={typeof value === 'string' ? value : ''}
            onChange={(e) => onChange(e.target.value)}
          />
          {typeof value === 'string' && value && (
            <img key={value} src={value} alt={altFromKey(value)} className={styles.preview} />
          )}
          {libraryOpen && (
            <MediaLibrary
              canDelete={canUpload}
              onClose={() => setLibraryOpen(false)}
              onSelect={(object) => {
                onChange(object.url);
                setLibraryOpen(false);
              }}
            />
          )}
        </div>
      );
    default:
      return (
        <input
          id={id}
          type={field.type === 'url' ? 'url' : 'text'}
          className={styles.input}
          placeholder={placeholder}
          value={typeof value === 'string' ? value : ''}
          onChange={(e) => onChange(e.target.value)}
        />
      );
  }
};

interface FieldRowProps extends FieldControlProps {
  /** A required field that was left empty */
  missing: boolean;
}

/** A field with its label, the input, the "required" warning and the help text */
const FieldRow: React.FC<FieldRowProps> = ({ missing, ...control }) => {
  const { t, i18n } = useTranslation();
  const { field, id } = control;
  return (
    <div className={field.type === 'boolean' ? styles.checkField : styles.field}>
      <label htmlFor={id} className={styles.label}>
        {localize(field.label, i18n.language)}
        {field.required && <span className={styles.required}> *</span>}
      </label>
      <FieldControl {...control} />
      {missing && <span className={styles.warning}>{t('blocks.plugin.required')}</span>}
      {field.help && <span className={styles.help}>{localize(field.help, i18n.language)}</span>}
    </div>
  );
};

/** Form of a plugin's custom block, built from the fields its manifest declares */
export const PluginBlockEditor: React.FC<PluginBlockEditorProps> = ({
  block,
  definition,
  onChange,
  canUpload,
}) => {
  const { t, i18n } = useTranslation();
  const missing = missingRequiredFields(definition, block);
  const description = localize(definition.description, i18n.language);

  const setValue = (name: string, value: unknown) => {
    const attributes = { ...block.attributes };
    if (value === undefined) {
      delete attributes[name];
    } else {
      attributes[name] = value;
    }
    onChange({ ...block, attributes });
  };

  return (
    <div className={styles.form}>
      <p className={styles.source}>
        {t('blocks.plugin.from', { plugin: definition.plugin_display_name })}
        {description && ` — ${description}`}
      </p>
      {definition.fields.map((field) => (
        <FieldRow
          key={field.name}
          field={field}
          id={`plugin-field-${field.name}`}
          value={block.attributes?.[field.name]}
          missing={missing.includes(field.name)}
          onChange={(value) => setValue(field.name, value)}
          canUpload={canUpload}
        />
      ))}
    </div>
  );
};
