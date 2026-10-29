import React, { useState } from 'react';
import { Images } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { BlockNode, PluginBlockDefinition, PluginBlockField } from '../types';
import { altFromKey } from '../utils/media';
import { localize, missingRequiredFields } from '../utils/pluginBlocks';
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

/** Form of a plugin's custom block, built from the fields its manifest declares */
export const PluginBlockEditor: React.FC<PluginBlockEditorProps> = ({
  block,
  definition,
  onChange,
  canUpload,
}) => {
  const { t, i18n } = useTranslation();
  const [libraryField, setLibraryField] = useState<string | null>(null);
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

  const input = (field: PluginBlockField) => {
    const id = `plugin-field-${field.name}`;
    const value = block.attributes?.[field.name];
    const placeholder = localize(field.placeholder, i18n.language);
    switch (field.type) {
      case 'text':
        return (
          <textarea
            id={id}
            className={styles.textarea}
            placeholder={placeholder}
            value={typeof value === 'string' ? value : ''}
            onChange={(e) => setValue(field.name, e.target.value)}
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
            onChange={(e) =>
              setValue(field.name, e.target.value === '' ? undefined : Number(e.target.value))
            }
          />
        );
      case 'boolean':
        return (
          <input
            id={id}
            type="checkbox"
            checked={value === true}
            onChange={(e) => setValue(field.name, e.target.checked)}
          />
        );
      case 'select':
        return (
          <select
            id={id}
            className={styles.input}
            value={typeof value === 'string' ? value : ''}
            onChange={(e) => setValue(field.name, e.target.value)}
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
            {canUpload && <ImageUploader onUploaded={(meta) => setValue(field.name, meta.url)} />}
            <Button className={styles.library} onClick={() => setLibraryField(field.name)}>
              <Images size={14} />
              <span>{t('blocks.library')}</span>
            </Button>
            <input
              id={id}
              type="text"
              className={styles.input}
              placeholder={placeholder || t('blocks.urlPlaceholder')}
              value={typeof value === 'string' ? value : ''}
              onChange={(e) => setValue(field.name, e.target.value)}
            />
            {typeof value === 'string' && value && (
              <img key={value} src={value} alt={altFromKey(value)} className={styles.preview} />
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
            onChange={(e) => setValue(field.name, e.target.value)}
          />
        );
    }
  };

  return (
    <div className={styles.form}>
      <p className={styles.source}>
        {t('blocks.plugin.from', { plugin: definition.plugin_display_name })}
        {description && ` — ${description}`}
      </p>
      {definition.fields.map((field) => (
        <div
          key={field.name}
          className={field.type === 'boolean' ? styles.checkField : styles.field}
        >
          <label htmlFor={`plugin-field-${field.name}`} className={styles.label}>
            {localize(field.label, i18n.language)}
            {field.required && <span className={styles.required}> *</span>}
          </label>
          {input(field)}
          {missing.includes(field.name) && (
            <span className={styles.warning}>{t('blocks.plugin.required')}</span>
          )}
          {field.help && <span className={styles.help}>{localize(field.help, i18n.language)}</span>}
        </div>
      ))}
      {libraryField && (
        <MediaLibrary
          canDelete={canUpload}
          onClose={() => setLibraryField(null)}
          onSelect={(object) => {
            setValue(libraryField, object.url);
            setLibraryField(null);
          }}
        />
      )}
    </div>
  );
};
