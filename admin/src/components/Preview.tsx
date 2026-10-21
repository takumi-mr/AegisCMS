import React, { useState } from 'react';
import { Eye, Code, ShieldAlert } from 'lucide-react';
import type { BlockNode } from '../types';

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
    <aside className="preview-column">
      <div className="preview-tabs">
        <div
          className={`preview-tab ${tab === 'html' ? 'active' : ''}`}
          onClick={() => setTab('html')}
        >
          <Eye size={14} style={{ display: 'inline', marginRight: '6px' }} />
          Live HTML Output
        </div>
        <div
          className={`preview-tab ${tab === 'ast' ? 'active' : ''}`}
          onClick={() => setTab('ast')}
        >
          <Code size={14} style={{ display: 'inline', marginRight: '6px' }} />
          Block AST (JSON)
        </div>
      </div>

      <div className="preview-content">
        {tab === 'html' ? (
          <div>
            <div style={{ marginBottom: '16px', borderBottom: '1px solid var(--border-subtle)', paddingBottom: '12px' }}>
              <h1 style={{ fontFamily: 'var(--font-heading)', fontSize: '1.6rem', color: '#fff' }}>
                {title || 'Untitled Article'}
              </h1>
              <div style={{ fontSize: '0.75rem', color: 'var(--accent-teal)', marginTop: '4px' }}>
                ✓ Deterministic HTML Stream Sanitized
              </div>
            </div>

            <div
              className="html-output"
              dangerouslySetInnerHTML={{ __html: renderedHtml }}
            />

            {blocks.length === 0 && (
              <div style={{ textAlign: 'center', padding: '48px 0', color: 'var(--text-muted)' }}>
                <ShieldAlert size={36} style={{ margin: '0 auto 8px', opacity: 0.4 }} />
                <p style={{ fontSize: '0.85rem' }}>Add blocks from the toolbar to see live deterministic output.</p>
              </div>
            )}
          </div>
        ) : (
          <pre
            style={{
              fontFamily: 'var(--font-mono)',
              fontSize: '0.82rem',
              color: 'var(--accent-cyan)',
              lineHeight: 1.5,
              whiteSpace: 'pre-wrap',
            }}
          >
            {JSON.stringify(blocks, null, 2)}
          </pre>
        )}
      </div>
    </aside>
  );
};
