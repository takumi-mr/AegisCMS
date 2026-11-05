import React from 'react';
import { useTranslation } from 'react-i18next';
import type { FieldDefinition } from '../types';
import type { ExtraFieldError, ExtraValue, ExtraValues } from '../utils/extraFields';
import { Field } from './ui/Field';
import styles from './ExtraFieldsEditor.module.css';

interface ExtraFieldsEditorProps {
  /** The schema fields without a dedicated input (see `extraFieldDefs`) */
  defs: FieldDefinition[];
  values: ExtraValues;
  /** The value that could not be saved, if any */
  error?: ExtraFieldError | null;
  disabled?: boolean;
  onChange: (name: string, value: ExtraValue) => void;
}

/**
 * Inputs for the schema fields that have no dedicated editor, chosen by the field's type.
 * Every input is a plain control: it edits the value, the schema decides what is saved.
 */
export const ExtraFieldsEditor: React.FC<ExtraFieldsEditorProps> = ({
  defs,
  values,
  error,
  disabled,
  onChange,
}) => {
  const { t } = useTranslation();
  if (defs.length === 0) return null;

  return (
    <>
      {defs.map((def) => {
        const id = `extra-field-${def.name}`;
        const value = values[def.name];
        const text = typeof value === 'string' ? value : '';
        const fieldError = error?.field === def.name ? error : null;
        const label = def.required ? `${def.name} *` : def.name;
        const common = {
          id,
          disabled,
          'aria-invalid': fieldError ? true : undefined,
        };

        let control: React.ReactNode;
        switch (def.type) {
          case 'boolean':
            control = (
              <input
                {...common}
                type="checkbox"
                checked={value === true}
                onChange={(e) => onChange(def.name, e.target.checked)}
              />
            );
            break;
          case 'text':
            control = (
              <textarea
                {...common}
                rows={4}
                value={text}
                onChange={(e) => onChange(def.name, e.target.value)}
              />
            );
            break;
          case 'jsonb':
            control = (
              <textarea
                {...common}
                rows={6}
                spellCheck={false}
                value={text}
                placeholder="{}"
                onChange={(e) => onChange(def.name, e.target.value)}
              />
            );
            break;
          case 'integer':
            control = (
              <input
                {...common}
                inputMode="numeric"
                value={text}
                onChange={(e) => onChange(def.name, e.target.value)}
              />
            );
            break;
          case 'timestamp':
            control = (
              <input
                {...common}
                type="datetime-local"
                value={text}
                onChange={(e) => onChange(def.name, e.target.value)}
              />
            );
            break;
          default:
            // string and relation (the id of the related content)
            control = (
              <input
                {...common}
                value={text}
                maxLength={def.max_length}
                placeholder={def.type === 'relation' ? t('extraFields.relationPlaceholder') : ''}
                onChange={(e) => onChange(def.name, e.target.value)}
              />
            );
        }

        return (
          <React.Fragment key={def.name}>
            <Field label={label} as="label">
              {control}
            </Field>
            {fieldError && (
              <span role="alert" className={styles.error}>
                {t(`extraFields.errors.${fieldError.code}`, { field: def.name })}
              </span>
            )}
          </React.Fragment>
        );
      })}
    </>
  );
};
