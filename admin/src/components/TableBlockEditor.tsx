import React from 'react';
import { Minus, Plus } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { BlockNode } from '../types';
import { tableRows } from '../utils/markdown';
import { Button } from './ui/Button';
import styles from './TableBlockEditor.module.css';

type Align = 'left' | 'center' | 'right' | null;

interface TableBlockEditorProps {
  block: BlockNode;
  onChange: (updated: BlockNode) => void;
}

/** Editor of a `core/table` block: a grid of cells, the header row and each column's alignment */
export const TableBlockEditor: React.FC<TableBlockEditorProps> = ({ block, onChange }) => {
  const { t } = useTranslation();
  const rows = tableRows(block);
  const columns = Math.max(1, ...rows.map((r) => r.length));
  // Every row as wide as the table
  const grid = (rows.length > 0 ? rows : [[]]).map((r) =>
    Array.from({ length: columns }, (_, c) => r[c] ?? '')
  );
  const header = block.attributes?.header !== false;
  const align: Align[] = Array.from(
    { length: columns },
    (_, c) => block.attributes?.align?.[c] ?? null
  );

  const update = (nextRows: string[][], nextAlign: Align[] = align, nextHeader = header) =>
    onChange({
      ...block,
      attributes: { ...block.attributes, rows: nextRows, align: nextAlign, header: nextHeader },
    });

  const setCell = (r: number, c: number, value: string) =>
    update(
      grid.map((row, ri) => (ri === r ? row.map((cell, ci) => (ci === c ? value : cell)) : row))
    );

  return (
    <div className={styles.editor}>
      <div className={styles.toolbar}>
        <label className={styles.check}>
          <input
            type="checkbox"
            checked={header}
            onChange={(e) => update(grid, align, e.target.checked)}
          />
          {t('blocks.tableHeader')}
        </label>
        <div className={styles.buttons}>
          <Button variant="tool" onClick={() => update([...grid, Array(columns).fill('')])}>
            <Plus size={12} />
            <span>{t('blocks.tableAddRow')}</span>
          </Button>
          <Button
            variant="tool"
            disabled={grid.length <= 1}
            onClick={() => update(grid.slice(0, -1))}
          >
            <Minus size={12} />
            <span>{t('blocks.tableRemoveRow')}</span>
          </Button>
          <Button
            variant="tool"
            onClick={() =>
              update(
                grid.map((row) => [...row, '']),
                [...align, null]
              )
            }
          >
            <Plus size={12} />
            <span>{t('blocks.tableAddColumn')}</span>
          </Button>
          <Button
            variant="tool"
            disabled={columns <= 1}
            onClick={() =>
              update(
                grid.map((row) => row.slice(0, -1)),
                align.slice(0, -1)
              )
            }
          >
            <Minus size={12} />
            <span>{t('blocks.tableRemoveColumn')}</span>
          </Button>
        </div>
      </div>

      <div className={styles.scroll}>
        <table className={styles.grid}>
          <thead>
            <tr>
              {align.map((a, c) => (
                <th key={c}>
                  <select
                    className={styles.align}
                    aria-label={t('blocks.tableAlign', { column: c + 1 })}
                    value={a ?? ''}
                    onChange={(e) =>
                      update(
                        grid,
                        align.map((x, i) => (i === c ? ((e.target.value || null) as Align) : x))
                      )
                    }
                  >
                    <option value="">{t('blocks.alignDefault')}</option>
                    <option value="left">{t('blocks.alignLeft')}</option>
                    <option value="center">{t('blocks.alignCenter')}</option>
                    <option value="right">{t('blocks.alignRight')}</option>
                  </select>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {grid.map((row, r) => (
              <tr key={r} className={header && r === 0 ? styles.headerRow : undefined}>
                {row.map((cell, c) => (
                  <td key={c}>
                    <input
                      type="text"
                      className={styles.cell}
                      aria-label={t('blocks.tableCell', { row: r + 1, column: c + 1 })}
                      value={cell}
                      style={{ textAlign: align[c] ?? undefined }}
                      onChange={(e) => setCell(r, c, e.target.value)}
                    />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className={styles.hint}>{t('blocks.inlineHint')}</p>
    </div>
  );
};
