import React, { useEffect, useId, useRef, useState } from 'react';
import { Plus, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type { Tag } from '../types';
import { cx } from '../utils/cx';
import styles from './TagPicker.module.css';

/** Wait this long after the last key press before looking up tags */
const LOOKUP_DELAY_MS = 200;
/** Suggestions shown at once */
const SUGGESTIONS = 8;

interface TagPickerProps {
  /** The tags put on the content */
  selected: Tag[];
  onChange: (tags: Tag[]) => void;
  /** False for viewers: picking existing tags works, creating a tag needs the editor role */
  canCreate?: boolean;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/**
 * Tags of a content: the chosen ones as chips, and a box to find a tag by name (or slug) and add it.
 * A name that matches no tag can be created on the spot. Nothing is saved here: the page saves the
 * chosen tags with the content
 */
export const TagPicker: React.FC<TagPickerProps> = ({
  selected,
  onChange,
  canCreate = true,
  onShowToast,
}) => {
  const { t } = useTranslation();
  const listId = useId();
  const [draft, setDraft] = useState<string>('');
  const [found, setFound] = useState<Tag[]>([]);
  const [open, setOpen] = useState<boolean>(false);
  const [active, setActive] = useState<number>(0);
  const [creating, setCreating] = useState<boolean>(false);
  // Only the newest lookup may show its answer
  const lookup = useRef(0);

  const term = draft.trim();
  useEffect(() => {
    if (!open) return;
    const requestId = ++lookup.current;
    const timer = setTimeout(() => {
      aegisApi
        .listTags({ q: term, limit: SUGGESTIONS + selected.length })
        .then((page) => {
          if (requestId === lookup.current) setFound(page.tags);
        })
        .catch((err) => console.warn('Could not look up tags:', err));
    }, LOOKUP_DELAY_MS);
    return () => clearTimeout(timer);
  }, [term, open, selected.length]);

  const chosen = new Set(selected.map((tag) => tag.id));
  const suggestions = found.filter((tag) => !chosen.has(tag.id)).slice(0, SUGGESTIONS);
  const exact = found.some((tag) => tag.name.toLowerCase() === term.toLowerCase());
  const canCreateNew = canCreate && term !== '' && !exact;
  // Entries of the list: the suggestions, then "create"
  const entries = suggestions.length + (canCreateNew ? 1 : 0);

  const add = (tag: Tag) => {
    if (!chosen.has(tag.id)) onChange([...selected, tag]);
    setDraft('');
    setActive(0);
  };

  const create = async () => {
    if (creating || !canCreateNew) return;
    setCreating(true);
    try {
      add(await aegisApi.createTag({ name: term }));
      setFound([]);
    } catch (err) {
      onShowToast(err instanceof Error ? err.message : t('tags.createFailed'), 'error');
    } finally {
      setCreating(false);
    }
  };

  const choose = (index: number) => {
    if (index < suggestions.length) add(suggestions[index]);
    else void create();
  };

  return (
    <div className={styles.picker}>
      <div className={styles.chips}>
        {selected.map((tag) => (
          <span key={tag.id} className={styles.chip}>
            {tag.name}
            <button
              type="button"
              className={styles.chipRemove}
              aria-label={t('tags.removeAria', { name: tag.name })}
              onClick={() => onChange(selected.filter((s) => s.id !== tag.id))}
            >
              <X size={12} />
            </button>
          </span>
        ))}
        {selected.length === 0 && <span className={styles.none}>{t('tags.noneSelected')}</span>}
      </div>
      <input
        className={styles.input}
        type="text"
        role="combobox"
        aria-expanded={open && entries > 0}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-label={t('tags.pickerAria')}
        placeholder={t('tags.pickerPlaceholder')}
        value={draft}
        maxLength={50}
        autoComplete="off"
        onChange={(e) => {
          setDraft(e.target.value);
          setActive(0);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown' && entries > 0) {
            e.preventDefault();
            setActive((active + 1) % entries);
          } else if (e.key === 'ArrowUp' && entries > 0) {
            e.preventDefault();
            setActive((active + entries - 1) % entries);
          } else if (e.key === 'Enter') {
            // Enter in the box never submits the page's form
            e.preventDefault();
            if (entries > 0) choose(Math.min(active, entries - 1));
          } else if (e.key === 'Escape') {
            setOpen(false);
          } else if (e.key === 'Backspace' && draft === '' && selected.length > 0) {
            onChange(selected.slice(0, -1));
          }
        }}
      />
      {open && entries > 0 && (
        <div className={styles.suggestions} role="listbox" id={listId}>
          {suggestions.map((tag, index) => (
            <button
              key={tag.id}
              type="button"
              role="option"
              aria-selected={index === active}
              className={cx(styles.suggestion, index === active && styles.active)}
              // The box would lose focus (closing the list) before the click lands
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => choose(index)}
            >
              <span>{tag.name}</span>
              <span className={styles.suggestionMeta}>
                {t('tags.usage', { count: tag.usage ?? 0 })}
              </span>
            </button>
          ))}
          {canCreateNew && (
            <button
              type="button"
              role="option"
              aria-selected={active === suggestions.length}
              className={cx(styles.suggestion, active === suggestions.length && styles.active)}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => choose(suggestions.length)}
            >
              <span>
                <Plus size={12} /> {t('tags.createNamed', { name: term })}
              </span>
            </button>
          )}
        </div>
      )}
    </div>
  );
};
