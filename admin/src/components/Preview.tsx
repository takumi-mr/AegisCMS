import React, { useState } from 'react';
import { Eye, Code, ShieldAlert } from 'lucide-react';
import type { BlockNode } from '../types';
import { cx } from '../utils/cx';
import styles from './Preview.module.css';

interface PreviewProps {
  title: string;
  blocks: BlockNode[];
  serverHtml?: string;
}

// クライアント側でも Rust 側 HtmlRenderer と完全に同一のルールで即座にプレビュー描画
function renderAstToHtml(nodes: BlockNode[]): string {
  return nodes
    .map((node) => {
      const escape = (str?: string) =>
        (str || '')
          .replace(/&/g, '&amp;')
          .replace(/</g, '&lt;')
          .replace(/>/g, '&gt;')
          .replace(/"/g, '&quot;');

      switch (node.name) {
        case 'core/heading': {
          const level = Math.min(Math.max(node.attributes?.level || 2, 1), 6);
          const slug = (node.text || '')
            .toLowerCase()
            .replace(/[^a-z0-9-]+/g, '-')
            .replace(/^-|-$/g, '');
          const idAttr = slug ? ` id="${escape(slug)}"` : '';
          return `<h${level}${idAttr}>${escape(node.text)}</h${level}>\n`;
        }
        case 'core/paragraph':
          return `<p>${escape(node.text)}</p>\n`;
        case 'core/code': {
          const lang = escape(node.attributes?.language || 'text');
          return `<pre><code class="language-${lang}">${escape(node.text)}</code></pre>\n`;
        }
        case 'core/image': {
          const src = escape(node.attributes?.src);
          const alt = escape(node.attributes?.alt);
          return `<figure><img src="${src}" alt="${alt}" loading="lazy" /></figure>\n`;
        }
        default:
          return `<div class="${escape(node.name)}">${escape(node.text)}</div>\n`;
      }
    })
    .join('');
}

export const Preview: React.FC<PreviewProps> = ({ title, blocks, serverHtml }) => {
  const [tab, setTab] = useState<'html' | 'ast'>('html');

  const renderedHtml = serverHtml || renderAstToHtml(blocks);

  return (
    <aside className={styles.column}>
      <div className={styles.tabs}>
        <div
          className={cx(styles.tab, tab === 'html' && styles.active)}
          onClick={() => setTab('html')}
        >
          <Eye size={14} className={styles.tabIcon} />
          Live HTML Output
        </div>
        <div
          className={cx(styles.tab, tab === 'ast' && styles.active)}
          onClick={() => setTab('ast')}
        >
          <Code size={14} className={styles.tabIcon} />
          Block AST (JSON)
        </div>
      </div>

      <div className={styles.content}>
        {tab === 'html' ? (
          <div>
            <div className={styles.titleBlock}>
              <h1 className={styles.title}>{title || 'Untitled Article'}</h1>
              <div className={styles.sanitized}>✓ Deterministic HTML Stream Sanitized</div>
            </div>

            <div className={styles.htmlOutput} dangerouslySetInnerHTML={{ __html: renderedHtml }} />

            {blocks.length === 0 && (
              <div className={styles.empty}>
                <ShieldAlert size={36} className={styles.emptyIcon} />
                <p>Add blocks from the toolbar to see live deterministic output.</p>
              </div>
            )}
          </div>
        ) : (
          <pre className={styles.ast}>{JSON.stringify(blocks, null, 2)}</pre>
        )}
      </div>
    </aside>
  );
};
