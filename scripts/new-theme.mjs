#!/usr/bin/env node
/**
 * scripts/new-theme.mjs
 *
 * AegisCMS テーマのひな形を生成するスクリプト。
 * 最小限かつ完全に動作する MiniJinja テンプレート群とアセットを即座に生成します。
 *
 * 使い方:
 *   node scripts/new-theme.mjs <theme-name> [options]
 *   task theme:new -- <theme-name> [--from <existing-theme>] [--title "My Theme"]
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');

const args = process.argv.slice(2);
let themeId = '';
let themeTitle = '';
let fromTheme = '';
let force = false;

for (let i = 0; i < args.length; i++) {
  const arg = args[i];
  if (arg === '--help' || arg === '-h') {
    printHelp();
    process.exit(0);
  } else if ((arg === '--from' || arg === '-f') && i + 1 < args.length) {
    fromTheme = args[++i];
  } else if ((arg === '--title' || arg === '-t') && i + 1 < args.length) {
    themeTitle = args[++i];
  } else if (arg === '--force') {
    force = true;
  } else if (!arg.startsWith('-') && !themeId) {
    themeId = arg;
  }
}

function printHelp() {
  console.log(`
AegisCMS Theme Generator

Usage:
  task theme:new -- <theme-name> [options]
  node scripts/new-theme.mjs <theme-name> [options]

Arguments:
  <theme-name>               ID and directory name of the theme (e.g. sunset, modern-blog).
                             Must contain only lowercase letters, numbers, and hyphens.

Options:
  --title, -t <title>        Display name of the theme (e.g. "Sunset Glow")
  --from, -f <theme>         Copy an existing theme (e.g. trailhead, roastery) as the starting point
  --force                    Overwrite target directory if it already exists
  --help, -h                 Show this help message

Examples:
  task theme:new -- modern-blog
  task theme:new -- coffee-craft --title "Coffee Craft"
  task theme:new -- custom-trailhead --from trailhead
`);
}

if (!themeId) {
  console.error('Error: Theme name is required.');
  printHelp();
  process.exit(1);
}

// バリデーション: 英小文字・数字・ハイフン
if (!/^[a-z0-9]+(-[a-z0-9]+)*$/.test(themeId)) {
  console.error(
    `Error: Invalid theme ID "${themeId}".\n` +
      'Theme ID must be lowercase alphanumeric characters and hyphens (e.g. "sunset", "modern-blog").'
  );
  process.exit(1);
}

// 表示名の自動生成 (kebab-case -> Title Case)
if (!themeTitle) {
  themeTitle = themeId
    .split('-')
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(' ');
}

const targetDir = path.join(rootDir, 'themes', themeId);

if (fs.existsSync(targetDir)) {
  if (!force) {
    console.error(
      `Error: Target directory already exists: ${path.relative(rootDir, targetDir)}\n` +
        'Use --force to overwrite.'
    );
    process.exit(1);
  }
  fs.rmSync(targetDir, { recursive: true, force: true });
}

fs.mkdirSync(targetDir, { recursive: true });

if (fromTheme) {
  // 既存テーマのコピー
  const sourceDir = path.join(rootDir, 'themes', fromTheme);
  if (!fs.existsSync(sourceDir)) {
    console.error(`Error: Source theme directory not found: ${sourceDir}`);
    process.exit(1);
  }

  function copyRecursive(src, dest) {
    const entries = fs.readdirSync(src, { withFileTypes: true });
    for (const entry of entries) {
      if (entry.name === '.git' || entry.name === 'node_modules') continue;
      const srcPath = path.join(src, entry.name);
      const destPath = path.join(dest, entry.name);
      if (entry.isDirectory()) {
        fs.mkdirSync(destPath, { recursive: true });
        copyRecursive(srcPath, destPath);
      } else {
        fs.copyFileSync(srcPath, destPath);
      }
    }
  }

  copyRecursive(sourceDir, targetDir);

  // theme.json の id と name を置換
  const manifestPath = path.join(targetDir, 'theme.json');
  if (fs.existsSync(manifestPath)) {
    try {
      const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf-8'));
      manifest.id = themeId;
      manifest.name = themeTitle;
      fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n', 'utf-8');
    } catch {
      // ignore
    }
  }
} else {
  // 新規スターターテンプレートの生成
  fs.mkdirSync(path.join(targetDir, 'templates'), { recursive: true });
  fs.mkdirSync(path.join(targetDir, 'assets'), { recursive: true });

  // 1. theme.json
  const themeManifest = {
    id: themeId,
    name: themeTitle,
    version: '1.0.0',
    description: `A clean, responsive, modern AegisCMS theme designed for ${themeTitle}.`,
    author: {
      name: 'AegisCMS Creator',
      url: 'https://example.com',
    },
    license: 'MIT',
    preview_image: 'preview.svg',
    tags: ['clean', 'responsive', 'modern'],
    min_aegis_version: '0.1.0',
  };
  fs.writeFileSync(
    path.join(targetDir, 'theme.json'),
    JSON.stringify(themeManifest, null, 2) + '\n',
    'utf-8'
  );

  // 2. preview.svg
  const previewSvg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 500" width="800" height="500">
  <defs>
    <linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#1e293b" />
      <stop offset="100%" stop-color="#0f172a" />
    </linearGradient>
    <linearGradient id="accent" x1="0%" y1="0%" x2="100%" y2="0%">
      <stop offset="0%" stop-color="#38bdf8" />
      <stop offset="100%" stop-color="#818cf8" />
    </linearGradient>
  </defs>
  <rect width="800" height="500" fill="url(#bg)" />
  <rect x="60" y="60" width="680" height="380" rx="16" fill="#1e293b" stroke="#334155" stroke-width="2" />
  <rect x="60" y="60" width="680" height="50" rx="16" fill="#0f172a" />
  <circle cx="95" cy="85" r="6" fill="#ef4444" />
  <circle cx="115" cy="85" r="6" fill="#f59e0b" />
  <circle cx="135" cy="85" r="6" fill="#10b981" />
  <text x="400" y="240" font-family="system-ui, -apple-system, sans-serif" font-size="32" font-weight="700" fill="url(#accent)" text-anchor="middle">${themeTitle}</text>
  <text x="400" y="280" font-family="system-ui, -apple-system, sans-serif" font-size="16" fill="#94a3b8" text-anchor="middle">AegisCMS Custom Theme</text>
</svg>
`;
  fs.writeFileSync(path.join(targetDir, 'preview.svg'), previewSvg, 'utf-8');

  // 3. templates/base.html
  const baseHtml = `<!doctype html>
<html lang="{{ locale | default('en') }}">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{% if title %}{{ title }} · {% endif %}{{ site_title }}</title>
  {% if meta_description %}<meta name="description" content="{{ meta_description }}">{% endif %}
  <link rel="stylesheet" href="{{ base_path }}/theme/assets/style.css">
  {{ extra_head | safe }}
</head>
<body>
  {% block notices %}{% endblock %}

  <header class="site-header">
    <div class="container header-inner">
      <a class="site-brand" href="{{ base_path }}/">{{ site_title }}</a>
      <nav class="site-nav">
        <a href="{{ base_path }}/">Home</a>
        <a href="{{ base_path }}/articles">Articles</a>
      </nav>
    </div>
  </header>

  <main class="site-main container">
    {% block content %}{% endblock %}
  </main>

  <footer class="site-footer">
    <div class="container footer-inner">
      <p>&copy; {{ site_title }}. Powered by AegisCMS.</p>
    </div>
  </footer>

  {% block consent %}{% endblock %}
  {{ extra_footer | safe }}
</body>
</html>
`;
  fs.writeFileSync(path.join(targetDir, 'templates', 'base.html'), baseHtml, 'utf-8');

  // 4. templates/index.html
  const indexHtml = `{% extends "base.html" %}

{% block content %}
<section class="posts-section">
  <header class="section-header">
    <h1 class="section-title">{{ title | default('Latest Articles') }}</h1>
  </header>

  <div class="posts-grid">
    {% for item in items %}
      <article class="post-card">
        {% if item.header_image %}
          <div class="post-card-thumb">
            <img src="{{ item.header_image }}" alt="{{ item.title }}" loading="lazy">
          </div>
        {% endif %}
        <div class="post-card-body">
          <h2 class="post-card-title">
            <a href="{{ item.url }}">{{ item.title }}</a>
          </h2>
          {% if item.published_at %}
            <time class="post-card-date">{{ item.published_at | date }}</time>
          {% endif %}
          {% if item.excerpt %}
            <p class="post-card-excerpt">{{ item.excerpt }}</p>
          {% endif %}
        </div>
      </article>
    {% else %}
      <p class="empty-message">No articles published yet.</p>
    {% endfor %}
  </div>

  {% if pagination and pagination.total_pages > 1 %}
    <nav class="pagination" aria-label="Pagination">
      {% if pagination.prev_url %}
        <a class="page-link" href="{{ pagination.prev_url }}">&larr; Previous</a>
      {% endif %}
      <span class="page-current">Page {{ pagination.current_page }} of {{ pagination.total_pages }}</span>
      {% if pagination.next_url %}
        <a class="page-link" href="{{ pagination.next_url }}">Next &rarr;</a>
      {% endif %}
    </nav>
  {% endif %}
</section>
{% endblock %}
`;
  fs.writeFileSync(path.join(targetDir, 'templates', 'index.html'), indexHtml, 'utf-8');

  // 5. templates/article.html
  const articleHtml = `{% extends "base.html" %}

{% block content %}
<article class="single-article">
  <header class="article-header">
    <h1 class="article-title">{{ title }}</h1>
    <div class="article-meta">
      {% if published_at %}
        <time datetime="{{ published_at }}">{{ published_at | date }}</time>
      {% endif %}
      {% if category %}
        <span class="meta-sep">&bull;</span>
        <a class="article-category" href="{{ category.url }}">{{ category.name }}</a>
      {% endif %}
    </div>
  </header>

  {% if header_image %}
    <figure class="article-cover">
      <img src="{{ header_image }}" alt="{{ title }}">
    </figure>
  {% endif %}

  <div class="article-content rich-content">
    {{ content_html | safe }}
  </div>

  {% if tags and tags | length > 0 %}
    <div class="article-tags">
      {% for tag in tags %}
        <a class="tag-pill" href="{{ tag.url }}">#{{ tag.name }}</a>
      {% endfor %}
    </div>
  {% endif %}
</article>
{% endblock %}
`;
  fs.writeFileSync(path.join(targetDir, 'templates', 'article.html'), articleHtml, 'utf-8');

  // 6. templates/page.html
  const pageHtml = `{% extends "base.html" %}

{% block content %}
<article class="single-page">
  <header class="page-header">
    <h1 class="page-title">{{ title }}</h1>
  </header>
  <div class="page-content rich-content">
    {{ content_html | safe }}
  </div>
</article>
{% endblock %}
`;
  fs.writeFileSync(path.join(targetDir, 'templates', 'page.html'), pageHtml, 'utf-8');

  // 7. templates/404.html
  const notFoundHtml = `{% extends "base.html" %}

{% block content %}
<div class="error-page">
  <h1 class="error-code">404</h1>
  <p class="error-message">The page you were looking for could not be found.</p>
  <a class="button" href="{{ base_path }}/">&larr; Return to Home</a>
</div>
{% endblock %}
`;
  fs.writeFileSync(path.join(targetDir, 'templates', '404.html'), notFoundHtml, 'utf-8');

  // 8. assets/style.css
  const styleCss = `:root {
  --color-bg: #fafafa;
  --color-surface: #ffffff;
  --color-text: #1e293b;
  --color-text-muted: #64748b;
  --color-primary: #0284c7;
  --color-primary-hover: #0369a1;
  --color-border: #e2e8f0;
  --max-width: 960px;
  --font-sans: system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

@media (prefers-color-scheme: dark) {
  :root {
    --color-bg: #0f172a;
    --color-surface: #1e293b;
    --color-text: #f1f5f9;
    --color-text-muted: #94a3b8;
    --color-primary: #38bdf8;
    --color-primary-hover: #7dd3fc;
    --color-border: #334155;
  }
}

* { box-sizing: border-box; margin: 0; padding: 0; }
body {
  font-family: var(--font-sans);
  background-color: var(--color-bg);
  color: var(--color-text);
  line-height: 1.7;
  display: flex;
  flex-direction: column;
  min-height: 100vh;
}

.container {
  max-width: var(--max-width);
  margin: 0 auto;
  padding: 0 1.5rem;
  width: 100%;
}

a { color: var(--color-primary); text-decoration: none; transition: color 0.15s; }
a:hover { color: var(--color-primary-hover); }

/* Header */
.site-header {
  background-color: var(--color-surface);
  border-bottom: 1px solid var(--color-border);
  padding: 1.25rem 0;
}
.header-inner { display: flex; align-items: center; justify-content: space-between; }
.site-brand { font-size: 1.25rem; font-weight: 700; color: var(--color-text); }
.site-nav { display: flex; gap: 1.25rem; }
.site-nav a { color: var(--color-text-muted); font-weight: 500; }
.site-nav a:hover { color: var(--color-primary); }

/* Main */
.site-main { flex: 1; padding-top: 2.5rem; padding-bottom: 3.5rem; }

/* Post List */
.section-title { font-size: 1.75rem; font-weight: 700; margin-bottom: 1.5rem; }
.posts-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 1.5rem; }
.post-card {
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 8px;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.post-card-thumb img { width: 100%; height: 180px; object-fit: cover; display: block; }
.post-card-body { padding: 1.25rem; flex: 1; display: flex; flex-direction: column; }
.post-card-title { font-size: 1.15rem; font-weight: 600; line-height: 1.4; margin-bottom: 0.5rem; }
.post-card-title a { color: var(--color-text); }
.post-card-title a:hover { color: var(--color-primary); }
.post-card-date { font-size: 0.85rem; color: var(--color-text-muted); margin-bottom: 0.75rem; }
.post-card-excerpt { font-size: 0.95rem; color: var(--color-text-muted); flex: 1; }

/* Article */
.single-article { max-width: 720px; margin: 0 auto; }
.article-header { margin-bottom: 2rem; }
.article-title { font-size: 2.25rem; font-weight: 800; line-height: 1.25; margin-bottom: 0.75rem; }
.article-meta { font-size: 0.9rem; color: var(--color-text-muted); display: flex; gap: 0.5rem; }
.article-cover img { width: 100%; border-radius: 8px; margin-bottom: 2rem; }
.rich-content > * + * { margin-top: 1.25rem; }
.rich-content h2 { font-size: 1.5rem; margin-top: 2rem; }
.rich-content h3 { font-size: 1.25rem; margin-top: 1.5rem; }
.rich-content pre { background: var(--color-surface); border: 1px solid var(--color-border); padding: 1rem; border-radius: 6px; overflow-x: auto; }
.article-tags { margin-top: 2.5rem; display: flex; gap: 0.5rem; flex-wrap: wrap; }
.tag-pill { background: var(--color-surface); border: 1px solid var(--color-border); padding: 0.25rem 0.75rem; border-radius: 999px; font-size: 0.85rem; }

/* Pagination */
.pagination { display: flex; align-items: center; justify-content: space-between; margin-top: 2.5rem; }

/* Error Page */
.error-page { text-align: center; padding: 4rem 1rem; }
.error-code { font-size: 5rem; font-weight: 900; color: var(--color-primary); }
.error-message { font-size: 1.25rem; color: var(--color-text-muted); margin: 1rem 0 2rem; }
.button { display: inline-block; background: var(--color-primary); color: #fff; padding: 0.6rem 1.25rem; border-radius: 6px; font-weight: 600; }
.button:hover { background: var(--color-primary-hover); color: #fff; }

/* Footer */
.site-footer {
  background-color: var(--color-surface);
  border-top: 1px solid var(--color-border);
  padding: 2rem 0;
  text-align: center;
  font-size: 0.875rem;
  color: var(--color-text-muted);
}
`;
  fs.writeFileSync(path.join(targetDir, 'assets', 'style.css'), styleCss, 'utf-8');
}

console.log(`\n🎉 Successfully created theme: ${themeId}`);
console.log(`   Location: ${path.relative(rootDir, targetDir)}`);
console.log(`   Title:    ${themeTitle}\n`);

console.log('Next steps:');
console.log(`  1. Develop with live reload:`);
console.log(`     task dev:theme -- themes/${themeId}`);
console.log(`\n  2. Or open Theme Builder UI:`);
console.log(`     task dev:theme-builder\n`);

