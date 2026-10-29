pub const BASE_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="ja">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{% if title %}{{ title }} | {% endif %}{{ site_title }}</title>
  <meta name="description" content="{% if site_description %}{{ site_description }}{% else %}AegisCMS Published Content{% endif %}">
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@400;500;600;700;800&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
  <style>
    :root {
      --bg-primary: #0a0e17;
      --bg-secondary: #111827;
      --bg-card: rgba(17, 24, 39, 0.7);
      --border-color: rgba(255, 255, 255, 0.08);
      --border-hover: rgba(99, 102, 241, 0.3);
      --text-main: #f3f4f6;
      --text-muted: #9ca3af;
      --text-dim: #6b7280;
      --accent: #6366f1;
      --accent-gradient: linear-gradient(135deg, #6366f1 0%, #a855f7 100%);
      --accent-glow: rgba(99, 102, 241, 0.15);
      --radius-sm: 8px;
      --radius-md: 14px;
      --radius-lg: 20px;
      --shadow-sm: 0 4px 6px -1px rgba(0, 0, 0, 0.2);
      --shadow-lg: 0 20px 25px -5px rgba(0, 0, 0, 0.4);
      --font-sans: 'Plus Jakarta Sans', system-ui, -apple-system, sans-serif;
      --font-mono: 'JetBrains Mono', monospace;
    }

    * {
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }

    body {
      background-color: var(--bg-primary);
      color: var(--text-main);
      font-family: var(--font-sans);
      line-height: 1.7;
      min-height: 100vh;
      display: flex;
      flex-direction: column;
      -webkit-font-smoothing: antialiased;
    }

    /* Ambient Background Glow */
    .ambient-bg {
      position: fixed;
      top: 0;
      left: 0;
      right: 0;
      height: 480px;
      background: radial-gradient(circle 800px at 50% -100px, rgba(99, 102, 241, 0.12), transparent);
      pointer-events: none;
      z-index: 0;
    }

    /* Header & Navigation */
    header.site-header {
      position: sticky;
      top: 0;
      z-index: 50;
      background: rgba(10, 14, 23, 0.85);
      backdrop-filter: blur(16px);
      -webkit-backdrop-filter: blur(16px);
      border-bottom: 1px solid var(--border-color);
    }

    .nav-container {
      max-width: 1100px;
      margin: 0 auto;
      padding: 0.9rem 1.5rem;
      display: flex;
      align-items: center;
      justify-content: space-between;
    }

    .brand-logo {
      display: flex;
      align-items: center;
      gap: 0.6rem;
      text-decoration: none;
      color: var(--text-main);
      font-weight: 700;
      font-size: 1.15rem;
      letter-spacing: -0.02em;
    }

    .logo-icon {
      width: 28px;
      height: 28px;
      background: var(--accent-gradient);
      border-radius: var(--radius-sm);
      display: flex;
      align-items: center;
      justify-content: center;
      box-shadow: 0 0 12px var(--accent-glow);
    }

    .logo-icon svg {
      width: 16px;
      height: 16px;
      fill: #ffffff;
    }

    .nav-links {
      display: flex;
      align-items: center;
      gap: 1.25rem;
    }

    .nav-link {
      color: var(--text-muted);
      text-decoration: none;
      font-size: 0.9rem;
      font-weight: 500;
      transition: color 0.2s ease;
    }

    .nav-link:hover {
      color: #ffffff;
    }

    .admin-pill {
      background: rgba(255, 255, 255, 0.05);
      border: 1px solid var(--border-color);
      padding: 0.35rem 0.85rem;
      border-radius: 9999px;
      font-size: 0.8rem;
      color: var(--text-muted);
      transition: all 0.2s ease;
    }

    .admin-pill:hover {
      background: rgba(99, 102, 241, 0.15);
      color: #ffffff;
      border-color: rgba(99, 102, 241, 0.4);
    }

    /* Main Container */
    main.site-main {
      flex: 1;
      max-width: 1100px;
      width: 100%;
      margin: 0 auto;
      padding: 2.5rem 1.5rem 4rem 1.5rem;
      position: relative;
      z-index: 1;
    }

    /* Typography / Prose for Articles & Pages */
    .prose {
      font-size: 1.05rem;
      color: #e5e7eb;
      line-height: 1.85;
    }

    .prose h1, .prose h2, .prose h3, .prose h4 {
      color: #ffffff;
      font-weight: 700;
      margin-top: 2rem;
      margin-bottom: 0.85rem;
      line-height: 1.35;
      letter-spacing: -0.02em;
    }

    .prose h1 { font-size: 2.25rem; }
    .prose h2 { font-size: 1.65rem; border-bottom: 1px solid var(--border-color); padding-bottom: 0.4rem; }
    .prose h3 { font-size: 1.3rem; }

    .prose p {
      margin-bottom: 1.35rem;
    }

    .prose a {
      color: #818cf8;
      text-decoration: underline;
      text-underline-offset: 3px;
      transition: color 0.15s ease;
    }

    .prose a:hover {
      color: #a5b4fc;
    }

    .prose ul, .prose ol {
      margin-bottom: 1.35rem;
      padding-left: 1.75rem;
    }

    .prose li {
      margin-bottom: 0.4rem;
    }

    .prose pre {
      background: #05070d;
      border: 1px solid var(--border-color);
      border-radius: var(--radius-md);
      padding: 1.25rem;
      overflow-x: auto;
      font-family: var(--font-mono);
      font-size: 0.9rem;
      margin-bottom: 1.5rem;
      line-height: 1.6;
    }

    .prose code {
      font-family: var(--font-mono);
      background: rgba(255, 255, 255, 0.08);
      padding: 0.15rem 0.4rem;
      border-radius: 4px;
      font-size: 0.9em;
      color: #cbd5e1;
    }

    .prose pre code {
      background: transparent;
      padding: 0;
      border-radius: 0;
      color: inherit;
    }

    .prose blockquote {
      border-left: 3px solid var(--accent);
      background: rgba(99, 102, 241, 0.05);
      padding: 0.75rem 1.25rem;
      border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
      margin-bottom: 1.5rem;
      color: var(--text-muted);
      font-style: italic;
    }

    .prose img {
      max-width: 100%;
      height: auto;
      border-radius: var(--radius-md);
      border: 1px solid var(--border-color);
      margin: 1.5rem 0;
      display: block;
    }

    /* Footer */
    footer.site-footer {
      border-top: 1px solid var(--border-color);
      background: rgba(10, 14, 23, 0.95);
      padding: 2.5rem 1.5rem;
      text-align: center;
      color: var(--text-dim);
      font-size: 0.85rem;
    }

    .footer-content {
      max-width: 1100px;
      margin: 0 auto;
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 0.75rem;
    }

    .footer-badge {
      display: inline-flex;
      align-items: center;
      gap: 0.4rem;
      background: rgba(255, 255, 255, 0.03);
      border: 1px solid var(--border-color);
      padding: 0.25rem 0.65rem;
      border-radius: 9999px;
      font-size: 0.75rem;
      color: var(--text-muted);
    }

    /* Plugin custom styles slot */
    {% if extra_css %}
    {{ extra_css | safe }}
    {% endif %}
  </style>
  {% if extra_head %}
  {{ extra_head | safe }}
  {% endif %}
</head>
<body>
  <div class="ambient-bg"></div>

  <header class="site-header">
    <div class="nav-container">
      <a href="{{ base_path }}/" class="brand-logo">
        <div class="logo-icon">
          <svg viewBox="0 0 24 24"><path d="M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm0 2.18l7 3.12v4.7c0 4.67-3.13 9.03-7 10.18-3.87-1.15-7-5.51-7-10.18V6.3l7-3.12z"/></svg>
        </div>
        <span>{{ site_title }}</span>
      </a>
      <nav class="nav-links">
        <a href="{{ base_path }}/" class="nav-link">Home</a>
        <a href="{{ base_path }}/admin/" class="admin-pill">Admin Studio</a>
      </nav>
    </div>
  </header>

  <main class="site-main">
    {% block content %}{% endblock %}
  </main>

  <footer class="site-footer">
    <div class="footer-content">
      <div class="footer-badge">
        <span>Powered by <strong>AegisCMS</strong></span>
        <span>•</span>
        <span>Zero-Trust WebAssembly Architecture</span>
      </div>
      <div>&copy; 2026 {{ site_title }}. All rights reserved.</div>
    </div>
  </footer>
</body>
</html>
"#;

pub const INDEX_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<section class="hero-section" style="margin-bottom: 3.5rem; text-align: center; padding: 2.5rem 1rem;">
  <div style="display: inline-block; background: rgba(99, 102, 241, 0.1); border: 1px solid rgba(99, 102, 241, 0.25); color: #a5b4fc; padding: 0.35rem 0.85rem; border-radius: 9999px; font-size: 0.8rem; font-weight: 600; margin-bottom: 1.25rem;">
    Zero-Trust Content Engine
  </div>
  <h1 style="font-size: 2.85rem; font-weight: 800; letter-spacing: -0.03em; margin-bottom: 1rem; background: linear-gradient(135deg, #ffffff 30%, #9ca3af 100%); -webkit-background-clip: text; -webkit-text-fill-color: transparent;">
    {{ site_title }}
  </h1>
  <p style="font-size: 1.15rem; color: var(--text-muted); max-width: 620px; margin: 0 auto; line-height: 1.6;">
    {{ site_description }}
  </p>
</section>

<section class="articles-section">
  <div style="display: flex; align-items: center; justify-content: space-between; margin-bottom: 1.75rem; border-bottom: 1px solid var(--border-color); padding-bottom: 0.75rem;">
    <h2 style="font-size: 1.35rem; font-weight: 700; color: #ffffff;">Latest Published Stories</h2>
    <span style="font-size: 0.85rem; color: var(--text-dim);">{% if pagination %}{{ pagination.total_items }}{% else %}{{ items | length }}{% endif %} posts</span>
  </div>

  {% if items and items | length > 0 %}
  <div class="cards-grid" style="display: grid; grid-template-columns: repeat(auto-fill, minmax(min(320px, 100%), 1fr)); gap: 1.75rem;">
    {% for item in items %}
    <article class="article-card" style="background: var(--bg-card); border: 1px solid var(--border-color); border-radius: var(--radius-md); padding: 1.5rem; display: flex; flex-direction: column; justify-content: space-between; transition: all 0.25s ease; position: relative; overflow: hidden; backdrop-filter: blur(8px);">
      <div>
        <div style="display: flex; align-items: center; gap: 0.6rem; margin-bottom: 0.85rem;">
          <span style="background: rgba(99, 102, 241, 0.15); color: #818cf8; font-size: 0.75rem; font-weight: 600; padding: 0.2rem 0.55rem; border-radius: var(--radius-sm); text-transform: uppercase;">
            {{ item.content_type }}
          </span>
          {% if item.published_at %}
          <time style="font-size: 0.8rem; color: var(--text-dim);" datetime="{{ item.published_at }}">
            {{ item.published_at | truncate(10, True, "") }}
          </time>
          {% endif %}
        </div>
        <h3 style="font-size: 1.25rem; font-weight: 700; margin-bottom: 0.65rem; line-height: 1.4;">
          <a href="{{ base_path }}/articles/{{ item.slug }}" style="color: #ffffff; text-decoration: none; transition: color 0.15s ease;">
            {{ item.title }}
          </a>
        </h3>
        {% if item.excerpt %}
        <p style="font-size: 0.92rem; color: var(--text-muted); line-height: 1.6; margin-bottom: 1.25rem;">
          {{ item.excerpt }}
        </p>
        {% endif %}
      </div>
      <div style="padding-top: 1rem; border-top: 1px solid var(--border-color); display: flex; align-items: center; justify-content: space-between;">
        <span style="font-size: 0.8rem; color: var(--text-dim);">
          {% if item.author_id %}By {{ item.author_id }}{% else %}Aegis Team{% endif %}
        </span>
        <a href="{{ base_path }}/articles/{{ item.slug }}" style="font-size: 0.85rem; font-weight: 600; color: #818cf8; text-decoration: none; display: flex; align-items: center; gap: 0.25rem;">
          Read story &rarr;
        </a>
      </div>
    </article>
    {% endfor %}
  </div>
  {% else %}
  <div style="text-align: center; padding: 4rem 1.5rem; background: var(--bg-card); border: 1px dashed var(--border-color); border-radius: var(--radius-lg);">
    <div style="width: 48px; height: 48px; background: rgba(255, 255, 255, 0.05); border-radius: 50%; display: flex; align-items: center; justify-content: center; margin: 0 auto 1rem auto; color: var(--text-dim);">
      &bull;
    </div>
    <h3 style="font-size: 1.2rem; font-weight: 600; margin-bottom: 0.5rem; color: #ffffff;">No Published Content Yet</h3>
    <p style="font-size: 0.9rem; color: var(--text-muted); margin-bottom: 1.5rem;">Create and publish articles from Admin Studio to see them listed here.</p>
    <a href="{{ base_path }}/admin/" class="admin-pill" style="display: inline-block; padding: 0.5rem 1.25rem;">Launch Admin Studio</a>
  </div>
  {% endif %}
{% if pagination and pagination.total_pages > 1 %}
<nav class="pagination" aria-label="Pagination" style="display: flex; align-items: center; justify-content: space-between; gap: 1rem; margin-top: 2.5rem; color: var(--text-dim); font-size: 0.85rem;">
  {% if pagination.prev_url %}<a href="{{ pagination.prev_url }}" rel="prev" class="admin-pill">&larr; Newer stories</a>{% else %}<span></span>{% endif %}
  <span>Page {{ pagination.page }} / {{ pagination.total_pages }}</span>
  {% if pagination.next_url %}<a href="{{ pagination.next_url }}" rel="next" class="admin-pill">Older stories &rarr;</a>{% else %}<span></span>{% endif %}
</nav>
{% endif %}
</section>
{% endblock %}
"#;

/// 本文のブロック（表・目次・注記・引用・折りたたみ）の最低限の見た目。全ページの `<head>` に入る。
/// `:where()` で詳細度を 0 にしているので、テーマの CSS の指定がいつも優先される。
/// 色は文字色の濃淡と、注記の種類ごとの `--aegis-callout`（テーマで上書きできる）だけを使う
pub const BLOCK_STYLES: &str = "<style id=\"aegis-block-styles\">\
:where(.aegis-table){overflow-x:auto;margin:1.5rem 0}\
:where(.aegis-table) table{border-collapse:collapse;width:100%}\
:where(.aegis-table) :is(th,td){border:1px solid rgba(127,127,127,.3);padding:.5rem .75rem;text-align:left;vertical-align:top;overflow-wrap:break-word}\
:where(.aegis-table) th{background:rgba(127,127,127,.08);font-weight:600}\
:where(.aegis-toc){border:1px solid rgba(127,127,127,.3);border-radius:8px;padding:1rem 1.25rem;margin:1.5rem 0}\
:where(.aegis-toc-title){font-weight:700;margin:0 0 .5rem}\
:where(.aegis-toc) ol{margin:0;padding-left:1.25rem}\
:where(.aegis-toc) ol ol{margin-top:.25rem}\
:where(.aegis-callout){--aegis-callout:#3b82f6;border-left:4px solid var(--aegis-callout);background:rgba(127,127,127,.08);background:color-mix(in srgb,var(--aegis-callout) 10%,transparent);border-radius:0 8px 8px 0;padding:.75rem 1rem;margin:1.5rem 0}\
:where(.aegis-callout)>:first-child{margin-top:0}\
:where(.aegis-callout)>:last-child{margin-bottom:0}\
:where(.aegis-callout-title){font-weight:700;color:var(--aegis-callout)}\
:where(.aegis-callout-tip){--aegis-callout:#10b981}\
:where(.aegis-callout-important){--aegis-callout:#8b5cf6}\
:where(.aegis-callout-warning){--aegis-callout:#f59e0b}\
:where(.aegis-callout-caution){--aegis-callout:#ef4444}\
:where(.aegis-details){border:1px solid rgba(127,127,127,.3);border-radius:8px;margin:1rem 0;padding:0 1rem}\
:where(.aegis-details)>summary{cursor:pointer;font-weight:600;padding:.75rem 0}\
:where(.aegis-details-body){padding-bottom:.75rem}\
:where(.aegis-details-body)>:first-child{margin-top:0}\
:where(.aegis-details-body)>:last-child{margin-bottom:0}\
:where(.aegis-details)+:where(.aegis-details){margin-top:-.5rem}\
:where(.aegis-quote){margin:1.5rem 0}\
:where(.aegis-quote) figcaption{opacity:.7;font-size:.9em;margin-top:.25rem}\
</style>";

/// `[theme].front_page = "template"` のトップページ。テーマに `front.html` が無ければ記事一覧と同じ表示にする
pub const FRONT_TEMPLATE: &str = r#"{% extends "index.html" %}"#;

/// カテゴリの記事一覧（`/category/{slug}`）。どのテーマでも読めるよう、色は文字色の濃淡だけで付ける
pub const CATEGORY_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<section class="aegis-archive" style="max-width: 820px; margin: 0 auto; padding: 1rem 0 3rem;">
  <p class="aegis-archive-label" style="font-size: 0.8rem; letter-spacing: 0.08em; text-transform: uppercase; opacity: 0.65; margin: 0 0 0.4rem;">Category</p>
  <h1 style="margin: 0 0 0.75rem;">{{ title }}</h1>
  {% if fields.description %}<p class="aegis-archive-description" style="opacity: 0.75; margin: 0 0 2rem;">{{ fields.description }}</p>{% endif %}
  <ul class="aegis-archive-list" style="list-style: none; padding: 0; margin: 0;">
    {% for item in items %}
    <li style="padding: 1rem 0; border-top: 1px solid rgba(127, 127, 127, 0.25);">
      {% if item.published_at %}<time datetime="{{ item.published_at }}" style="font-size: 0.85rem; opacity: 0.65;">{{ item.published_at | date_short }}</time>{% endif %}
      <h2 style="font-size: 1.15rem; margin: 0.2rem 0 0.3rem;"><a href="{{ base_path }}/articles/{{ item.slug }}" style="color: inherit;">{{ item.title }}</a></h2>
      {% if item.excerpt %}<p style="opacity: 0.75; margin: 0;">{{ item.excerpt }}</p>{% endif %}
    </li>
    {% else %}
    <li style="padding: 1rem 0; border-top: 1px solid rgba(127, 127, 127, 0.25); opacity: 0.7;">No published articles in this category yet.</li>
    {% endfor %}
  </ul>
  {% if pagination and pagination.total_pages > 1 %}
  <nav class="pagination" aria-label="Pagination" style="display: flex; justify-content: space-between; gap: 1rem; margin-top: 2rem; font-size: 0.9rem;">
    {% if pagination.prev_url %}<a href="{{ pagination.prev_url }}" rel="prev">&larr; Newer</a>{% else %}<span></span>{% endif %}
    <span style="opacity: 0.65;">{{ pagination.page }} / {{ pagination.total_pages }}</span>
    {% if pagination.next_url %}<a href="{{ pagination.next_url }}" rel="next">Older &rarr;</a>{% else %}<span></span>{% endif %}
  </nav>
  {% endif %}
  <p style="margin-top: 2rem; font-size: 0.85rem;"><a href="{{ base_path }}/category/{{ slug }}/feed.xml">Feed (Atom)</a></p>
</section>
{% endblock %}
"#;

pub const ARTICLE_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<article class="article-detail" style="max-width: 820px; margin: 0 auto;">
  <nav class="breadcrumb" style="display: flex; align-items: center; gap: 0.5rem; font-size: 0.85rem; color: var(--text-dim); margin-bottom: 1.75rem;">
    <a href="{{ base_path }}/" style="color: var(--text-dim); text-decoration: none;">Home</a>
    <span>/</span>
    <span style="text-transform: capitalize;">{{ content_type }}</span>
    <span>/</span>
    <span style="color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{{ title }}</span>
  </nav>

  <header class="article-header" style="margin-bottom: 2.5rem; border-bottom: 1px solid var(--border-color); padding-bottom: 1.75rem;">
    <div style="display: flex; align-items: center; gap: 0.75rem; margin-bottom: 1rem;">
      <span style="background: rgba(99, 102, 241, 0.15); color: #818cf8; font-size: 0.78rem; font-weight: 600; padding: 0.25rem 0.65rem; border-radius: var(--radius-sm); text-transform: uppercase;">
        {{ content_type }}
      </span>
      {% if published_at %}
      <time style="font-size: 0.85rem; color: var(--text-dim);" datetime="{{ published_at }}">
        Published on {{ published_at | date_short }}
      </time>
      {% endif %}
    </div>

    <h1 style="font-size: 2.65rem; font-weight: 800; letter-spacing: -0.03em; line-height: 1.25; color: #ffffff; margin-bottom: 1.25rem;">
      {{ title }}
    </h1>

    <div style="display: flex; align-items: center; gap: 1rem;">
      <div style="width: 36px; height: 36px; border-radius: 50%; background: var(--accent-gradient); display: flex; align-items: center; justify-content: center; font-weight: 700; font-size: 0.85rem; color: #ffffff;">
        {% if author %}{{ author | truncate(1, True, "") | upper }}{% else %}A{% endif %}
      </div>
      <div>
        <div style="font-size: 0.9rem; font-weight: 600; color: #ffffff;">
          {% if author %}{{ author }}{% else %}Aegis Author{% endif %}
        </div>
        <div style="font-size: 0.78rem; color: var(--text-dim);">Verified Creator</div>
      </div>
    </div>
  </header>

  <div class="prose">
    {{ content_html | safe }}
  </div>

  {% if comments %}{% include "comments.html" %}{% endif %}

  <footer class="article-footer" style="margin-top: 3.5rem; padding-top: 2rem; border-top: 1px solid var(--border-color); display: flex; align-items: center; justify-content: space-between;">
    <a href="{{ base_path }}/" style="color: #818cf8; font-size: 0.9rem; text-decoration: none; font-weight: 600;">
      &larr; Back to all stories
    </a>
    <button onclick="window.scrollTo({ top: 0, behavior: 'smooth' })" style="background: transparent; border: 1px solid var(--border-color); color: var(--text-muted); padding: 0.4rem 0.85rem; border-radius: var(--radius-sm); font-size: 0.8rem; cursor: pointer; transition: all 0.2s ease;">
      Scroll to top &uarr;
    </button>
  </footer>
</article>
{% endblock %}
"#;

pub const PAGE_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<article class="page-detail" style="max-width: 820px; margin: 0 auto;">
  <header style="margin-bottom: 2rem; border-bottom: 1px solid var(--border-color); padding-bottom: 1.25rem;">
    <h1 style="font-size: 2.5rem; font-weight: 800; letter-spacing: -0.02em; color: #ffffff;">
      {{ title }}
    </h1>
  </header>

  <div class="prose">
    {{ content_html | safe }}
  </div>
  {% if comments %}{% include "comments.html" %}{% endif %}
</article>
{% endblock %}
"#;

pub const BLANK_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="ja">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{% if title %}{{ title }} | {% endif %}{{ site_title }}</title>
  <meta name="description" content="{% if site_description %}{{ site_description }}{% else %}AegisCMS Published Content{% endif %}">
  <style>
    * {
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }
    body {
      min-height: 100vh;
      line-height: 1.6;
      -webkit-font-smoothing: antialiased;
    }
    {% if extra_css %}
    {{ extra_css | safe }}
    {% endif %}
  </style>
  {% if extra_head %}
  {{ extra_head | safe }}
  {% endif %}
</head>
<body>
  {{ content_html | safe }}
</body>
</html>
"#;

pub const NOT_FOUND_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<div style="text-align: center; padding: 5rem 1rem;">
  <div style="font-size: 5rem; font-weight: 900; background: var(--accent-gradient); -webkit-background-clip: text; -webkit-text-fill-color: transparent; margin-bottom: 1rem; line-height: 1;">
    404
  </div>
  <h2 style="font-size: 1.75rem; font-weight: 700; color: #ffffff; margin-bottom: 0.75rem;">Page Not Found</h2>
  <p style="font-size: 1rem; color: var(--text-muted); max-width: 480px; margin: 0 auto 2rem auto;">
    The page or article you are looking for does not exist, has been archived, or is in draft status.
  </p>
  <a href="{{ base_path }}/" style="background: var(--accent-gradient); color: #ffffff; text-decoration: none; padding: 0.65rem 1.5rem; border-radius: var(--radius-sm); font-weight: 600; font-size: 0.95rem; display: inline-block;">
    Return to Homepage
  </a>
</div>
{% endblock %}
"#;

/// ショップのページ共通のスタイル・ナビゲーション・カートのスクリプト（各ショップテンプレートに含める）。
/// どのテーマの `base.html` の中でも読めるよう、色はテーマの文字色を基準にした半透明で指定する
pub const SHOP_COMMON_TEMPLATE: &str = r#"<style>
  .aegis-shop { max-width: 1100px; margin: 0 auto; }
  .aegis-shop a { color: inherit; }
  .aegis-shop-nav { display: flex; justify-content: space-between; align-items: center; gap: 1rem; margin-bottom: 2rem; padding-bottom: 0.75rem; border-bottom: 1px solid rgba(127,127,127,0.25); font-size: 0.9rem; }
  .aegis-shop-nav a { text-decoration: none; opacity: 0.85; }
  .aegis-shop-nav a:hover { opacity: 1; }
  .aegis-cart-link { display: inline-flex; align-items: center; gap: 0.4rem; font-weight: 600; }
  .aegis-badge { min-width: 1.4rem; padding: 0 0.4rem; border-radius: 999px; background: var(--accent, #6366f1); color: #fff; font-size: 0.75rem; text-align: center; }
  .aegis-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(230px, 100%), 1fr)); gap: 1.5rem; }
  .aegis-card { display: flex; flex-direction: column; border: 1px solid rgba(127,127,127,0.25); border-radius: 14px; overflow: hidden; background: rgba(127,127,127,0.06); text-decoration: none; }
  .aegis-card img, .aegis-card .aegis-noimg { width: 100%; aspect-ratio: 4 / 3; object-fit: cover; background: rgba(127,127,127,0.12); }
  .aegis-card-body { padding: 1rem; display: flex; flex-direction: column; gap: 0.35rem; flex: 1; }
  .aegis-card-title { font-weight: 700; font-size: 1.05rem; }
  .aegis-price { font-weight: 700; font-size: 1.05rem; }
  .aegis-compare { text-decoration: line-through; opacity: 0.55; font-size: 0.85rem; margin-left: 0.4rem; }
  .aegis-muted { opacity: 0.65; font-size: 0.85rem; }
  .aegis-soldout { color: #f43f5e; font-weight: 600; font-size: 0.85rem; }
  .aegis-product { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 2.5rem; align-items: start; }
  @media (max-width: 760px) { .aegis-product { grid-template-columns: 1fr; } }
  .aegis-product-image { width: 100%; border-radius: 16px; border: 1px solid rgba(127,127,127,0.25); }
  .aegis-product h1 { font-size: 2.1rem; line-height: 1.25; margin-bottom: 0.75rem; }
  /* テーマが * { margin: 0; padding: 0 } で消した本文の余白・リストの字下げを戻す（:where で詳細度 0 にし、テーマ側の指定を優先） */
  :where(.aegis-product .prose) > * + * { margin-top: 1rem; }
  :where(.aegis-product .prose) :is(h2, h3) { margin-top: 1.75rem; }
  :where(.aegis-product .prose) :is(ul, ol) { padding-left: 1.4rem; }
  .aegis-buy { display: flex; gap: 0.75rem; align-items: center; margin: 1.5rem 0; flex-wrap: wrap; }
  .aegis-qty { width: 4.5rem; padding: 0.55rem 0.6rem; border-radius: 10px; border: 1px solid rgba(127,127,127,0.35); background: transparent; color: inherit; font: inherit; }
  .aegis-button { display: inline-block; padding: 0.65rem 1.4rem; border-radius: 10px; border: none; background: var(--accent, #6366f1); color: #fff !important; font: inherit; font-weight: 600; text-decoration: none; cursor: pointer; }
  .aegis-button[disabled] { opacity: 0.5; cursor: not-allowed; }
  .aegis-button.aegis-secondary { background: transparent; color: inherit !important; border: 1px solid rgba(127,127,127,0.4); }
  .aegis-link { background: none; border: none; color: inherit; opacity: 0.7; cursor: pointer; text-decoration: underline; font: inherit; font-size: 0.85rem; }
  .aegis-cart-lines { display: flex; flex-direction: column; border: 1px solid rgba(127,127,127,0.25); border-radius: 14px; }
  .aegis-cart-line { display: grid; grid-template-columns: 1fr auto auto 7rem auto; gap: 1rem; align-items: center; padding: 0.9rem 1rem; border-bottom: 1px solid rgba(127,127,127,0.18); }
  .aegis-cart-line:last-child { border-bottom: none; }
  .aegis-cart-line strong { text-align: right; }
  .aegis-line-title { font-weight: 600; text-decoration: none; }
  .aegis-coupon { display: flex; gap: 0.5rem; margin: 1.25rem 0; max-width: 420px; }
  .aegis-coupon input, .aegis-field input { flex: 1; min-width: 0; padding: 0.6rem 0.75rem; border-radius: 10px; border: 1px solid rgba(127,127,127,0.35); background: transparent; color: inherit; font: inherit; }
  .aegis-summary { margin: 1rem 0 1.5rem; padding: 1rem 1.25rem; border-radius: 14px; background: rgba(127,127,127,0.07); display: flex; flex-direction: column; gap: 0.4rem; }
  .aegis-sum-row { display: flex; justify-content: space-between; gap: 1rem; }
  .aegis-discount { color: #10b981; }
  .aegis-total { margin-top: 0.4rem; padding-top: 0.6rem; border-top: 1px solid rgba(127,127,127,0.25); font-size: 1.15rem; }
  .aegis-checkout { display: grid; grid-template-columns: 1fr 1fr; gap: 0.9rem 1rem; max-width: 720px; }
  .aegis-checkout h2, .aegis-checkout button { grid-column: 1 / -1; }
  .aegis-checkout button { justify-self: start; }
  @media (max-width: 640px) { .aegis-checkout { grid-template-columns: 1fr; } .aegis-cart-line { grid-template-columns: 1fr auto; } }
  .aegis-field { display: flex; flex-direction: column; gap: 0.3rem; font-size: 0.85rem; }
  .aegis-field span { opacity: 0.75; }
  .aegis-error { color: #f43f5e; margin: 0.75rem 0; }
  .aegis-empty { text-align: center; padding: 3rem 1rem; border: 1px dashed rgba(127,127,127,0.35); border-radius: 16px; }
  .aegis-empty p { margin-bottom: 1rem; }
  .aegis-order-head { display: flex; justify-content: space-between; align-items: center; gap: 1rem; margin-bottom: 1rem; }
  .aegis-status { padding: 0.3rem 0.8rem; border-radius: 999px; font-size: 0.85rem; font-weight: 600; background: rgba(127,127,127,0.15); }
  .aegis-status-paid, .aegis-status-shipped, .aegis-status-completed { background: rgba(16,185,129,0.15); color: #10b981; }
  .aegis-status-pending_payment { background: rgba(245,158,11,0.15); color: #f59e0b; }
  .aegis-status-cancelled { background: rgba(244,63,94,0.15); color: #f43f5e; }
  .aegis-instructions, .aegis-address { margin: 1rem 0; padding: 1rem 1.25rem; border-radius: 14px; border: 1px solid rgba(127,127,127,0.25); }
  .aegis-instructions p, .aegis-address p { white-space: pre-line; margin-top: 0.4rem; }
</style>
<nav class="aegis-shop-nav" aria-label="Shop">
  <a href="{{ theme_vars.products_url }}">All products</a>
  <a href="{{ theme_vars.cart_url }}" class="aegis-cart-link">Cart <span class="aegis-badge" data-aegis-cart-count hidden>0</span></a>
</nav>
<script src="{{ theme_vars.cart_script }}" data-api="{{ theme_vars.shop_api }}" data-currency="{{ theme_vars.currency }}" data-cart-url="{{ theme_vars.cart_url }}" data-products-url="{{ theme_vars.products_url }}" data-shop-enabled="{{ theme_vars.shop_enabled }}" defer></script>"#;

/// コメント欄（`[comments]`）。`article.html` / `page.html` / `product.html` が
/// `{% if comments %}{% include "comments.html" %}{% endif %}` で読み込む。どのテーマの中でも読めるよう、
/// 色はテーマの文字色を基準にした半透明で指定する。投稿はスクリプトで JSON を送る
pub const COMMENTS_TEMPLATE: &str = r##"<section class="aegis-comments" id="comments">
<style>
  .aegis-comments { margin-top: 3rem; padding-top: 2rem; border-top: 1px solid rgba(127,127,127,0.25); }
  .aegis-comments h2 { font-size: 1.35rem; margin-bottom: 1.25rem; }
  .aegis-comments h3 { font-size: 1.1rem; margin-bottom: 0.75rem; }
  .aegis-comment { margin: 0 0 1rem calc(min(var(--depth, 0), 4) * 1.5rem); padding: 0.9rem 1rem; border-radius: 12px; background: rgba(127,127,127,0.07); border: 1px solid rgba(127,127,127,0.18); }
  .aegis-comment:target { border-color: var(--accent, #6366f1); }
  .aegis-comment-head { display: flex; flex-wrap: wrap; align-items: baseline; gap: 0.5rem; margin-bottom: 0.4rem; font-size: 0.9rem; }
  .aegis-comment-avatar { width: 28px; height: 28px; border-radius: 50%; align-self: center; object-fit: cover; }
  .aegis-comment-head time { opacity: 0.6; font-size: 0.8rem; }
  .aegis-comment-badge { padding: 0.05rem 0.5rem; border-radius: 999px; background: var(--accent, #6366f1); color: #fff; font-size: 0.7rem; font-weight: 600; }
  .aegis-comment-body { overflow-wrap: anywhere; }
  .aegis-comment-body p + p { margin-top: 0.6rem; }
  .aegis-comment-reply, .aegis-comment-cancel { margin-top: 0.4rem; background: none; border: none; padding: 0; color: inherit; opacity: 0.7; cursor: pointer; font: inherit; font-size: 0.8rem; text-decoration: underline; }
  .aegis-comment-form { display: grid; gap: 0.8rem; max-width: 640px; margin-top: 2rem; }
  .aegis-comment-form label { display: flex; flex-direction: column; gap: 0.3rem; font-size: 0.85rem; }
  .aegis-comment-form input:not([type=checkbox]), .aegis-comment-form textarea { padding: 0.6rem 0.75rem; border-radius: 10px; border: 1px solid rgba(127,127,127,0.35); background: transparent; color: inherit; font: inherit; }
  .aegis-comment-form .aegis-comment-check { flex-direction: row; align-items: center; gap: 0.5rem; }
  .aegis-comment-form button[type=submit] { justify-self: start; padding: 0.6rem 1.3rem; border-radius: 10px; border: none; background: var(--accent, #6366f1); color: #fff; font: inherit; font-weight: 600; cursor: pointer; }
  .aegis-comment-form button[disabled] { opacity: 0.5; cursor: wait; }
  .aegis-comment-hp { position: absolute; left: -10000px; width: 1px; height: 1px; overflow: hidden; }
  .aegis-comment-status:empty { display: none; }
  .aegis-comment-status[data-kind=error] { color: #f43f5e; }
  .aegis-comments-note { opacity: 0.65; font-size: 0.85rem; }
</style>
  <h2>{% if comments.count == 1 %}1 comment{% else %}{{ comments.count }} comments{% endif %}</h2>
  {% for c in comments["items"] %}
  <article class="aegis-comment" id="comment-{{ c.id }}" style="--depth: {{ c.depth }}">
    <header class="aegis-comment-head">
      {% if c.avatar_url %}<img class="aegis-comment-avatar" src="{{ c.avatar_url }}" alt="" width="28" height="28" loading="lazy" referrerpolicy="no-referrer">{% endif %}
      <strong>{{ c.author_name }}</strong>
      {% if c.staff %}<span class="aegis-comment-badge">Staff</span>{% endif %}
      <time datetime="{{ c.created_at }}">{{ c.created_at | date_short }}</time>
    </header>
    <div class="aegis-comment-body">{{ c.body_html | safe }}</div>
    {% if comments.open and c.depth < comments.max_depth %}<button type="button" class="aegis-comment-reply" data-reply-to="{{ c.id }}" data-reply-name="{{ c.author_name }}">Reply</button>{% endif %}
  </article>
  {% else %}
  <p class="aegis-comments-note">No comments yet.</p>
  {% endfor %}
  {% if comments.open %}
  <form class="aegis-comment-form" data-post-url="{{ comments.post_url }}">
    <h3>Leave a comment</h3>
    <p class="aegis-comments-note" data-replying hidden>Replying to <strong data-replying-name></strong> <button type="button" class="aegis-comment-cancel" data-cancel-reply>Cancel</button></p>
    <input type="hidden" name="parent_id" value="">
    <label>Name <input name="name" required maxlength="100" autocomplete="name"></label>
    <label>Email (not published) <input type="email" name="email" required maxlength="254" autocomplete="email"></label>
    <label>Comment <textarea name="body" required rows="5" maxlength="{{ comments.max_length }}"></textarea></label>
    <div class="aegis-comment-hp" aria-hidden="true"><label>Leave this empty <input name="website" tabindex="-1" autocomplete="off"></label></div>
    <label class="aegis-comment-check"><input type="checkbox" name="notify_replies"> Email me when someone replies or my comment is approved</label>
    <button type="submit">Post comment</button>
    <p class="aegis-comment-status" role="status" aria-live="polite"></p>
  </form>
  <script>
  (function () {
    var form = document.currentScript.previousElementSibling;
    var status = form.querySelector(".aegis-comment-status");
    var replying = form.querySelector("[data-replying]");
    var KEY = "aegis-comment-author";
    function show(text, kind) { status.textContent = text; status.dataset.kind = kind || ""; }
    function replyTo(id, name) {
      form.elements.parent_id.value = id || "";
      replying.hidden = !id;
      form.querySelector("[data-replying-name]").textContent = name || "";
    }
    try {
      var saved = JSON.parse(localStorage.getItem(KEY) || "{}");
      if (saved.name) form.elements.name.value = saved.name;
      if (saved.email) form.elements.email.value = saved.email;
    } catch (e) {}
    document.querySelectorAll(".aegis-comment-reply").forEach(function (button) {
      button.addEventListener("click", function () {
        replyTo(button.dataset.replyTo, button.dataset.replyName);
        form.scrollIntoView({ behavior: "smooth", block: "start" });
        form.elements.body.focus({ preventScroll: true });
      });
    });
    form.querySelector("[data-cancel-reply]").addEventListener("click", function () { replyTo(); });
    form.addEventListener("submit", function (event) {
      event.preventDefault();
      if (!form.reportValidity()) return;
      var f = form.elements;
      var submit = form.querySelector("button[type=submit]");
      submit.disabled = true;
      show("Sending\u2026");
      fetch(form.dataset.postUrl, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: f.name.value, email: f.email.value, body: f.body.value,
          parent_id: f.parent_id.value || null, notify_replies: f.notify_replies.checked,
          website: f.website.value
        })
      }).then(function (res) {
        return res.json().catch(function () { return {}; }).then(function (data) {
          if (!res.ok) throw new Error(data.error || "Your comment could not be posted. Please try again.");
          try { localStorage.setItem(KEY, JSON.stringify({ name: f.name.value, email: f.email.value })); } catch (e) {}
          if (data.status === "approved" && data.comment) {
            location.hash = "comment-" + data.comment.id;
            location.reload();
            return;
          }
          f.body.value = "";
          replyTo();
          show("Thank you! Your comment will appear once it has been approved.");
        });
      }).catch(function (e) { show(e.message, "error"); })
        .finally(function () { submit.disabled = false; });
    });
  })();
  </script>
  {% else %}
  <p class="aegis-comments-note">Comments are closed.</p>
  {% endif %}
</section>"##;

pub const PRODUCTS_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<section class="aegis-shop">
  {% include "shop_common.html" %}
  <h1 style="font-size: 2rem; margin-bottom: 1.5rem;">{{ title }}</h1>
  {% if items and items | length > 0 %}
  <div class="aegis-grid">
    {% for item in items %}
    <a class="aegis-card" href="{{ theme_vars.products_url }}/{{ item.slug }}">
      {% if item.fields.image_url %}<img src="{{ item.fields.image_url }}" alt="" loading="lazy">{% else %}<div class="aegis-noimg"></div>{% endif %}
      <div class="aegis-card-body">
        <span class="aegis-card-title">{{ item.title }}</span>
        {% if item.fields.excerpt %}<span class="aegis-muted">{{ item.fields.excerpt | truncate(80) }}</span>{% elif item.excerpt %}<span class="aegis-muted">{{ item.excerpt | truncate(80) }}</span>{% endif %}
        <span style="margin-top: auto;">
          <span class="aegis-price">{{ item.fields.price | money(theme_vars.currency) }}</span>
          {% if item.fields.compare_at_price %}<span class="aegis-compare">{{ item.fields.compare_at_price | money(theme_vars.currency) }}</span>{% endif %}
        </span>
        {% if item.fields.stock is defined and item.fields.stock is not none and item.fields.stock | int <= 0 %}<span class="aegis-soldout">Sold out</span>{% endif %}
      </div>
    </a>
    {% endfor %}
  </div>
  {% else %}
  <div class="aegis-empty"><p>No products yet.</p></div>
  {% endif %}
  {% if pagination and pagination.total_pages > 1 %}
  <nav aria-label="Pagination" style="display: flex; justify-content: space-between; margin-top: 2rem;">
    {% if pagination.prev_url %}<a href="{{ pagination.prev_url }}" class="aegis-button aegis-secondary">&larr; Previous</a>{% else %}<span></span>{% endif %}
    <span class="aegis-muted">Page {{ pagination.page }} / {{ pagination.total_pages }}</span>
    {% if pagination.next_url %}<a href="{{ pagination.next_url }}" class="aegis-button aegis-secondary">Next &rarr;</a>{% else %}<span></span>{% endif %}
  </nav>
  {% endif %}
</section>
{% endblock %}
"#;

pub const PRODUCT_TEMPLATE: &str = r##"{% extends "base.html" %}

{% block content %}
<section class="aegis-shop">
  {% include "shop_common.html" %}
  <article class="aegis-product">
    <div>
      {% if fields.image_url %}<img class="aegis-product-image" src="{{ fields.image_url }}" alt="{{ title }}">{% endif %}
    </div>
    <div>
      {% if relations.category_id %}<div class="aegis-muted">{{ relations.category_id.title }}</div>{% endif %}
      <h1>{{ title }}</h1>
      <div>
        <span class="aegis-price" style="font-size: 1.5rem;">{{ fields.price | money(theme_vars.currency) }}</span>
        {% if fields.compare_at_price %}<span class="aegis-compare">{{ fields.compare_at_price | money(theme_vars.currency) }}</span>{% endif %}
      </div>
      {% if fields.sku %}<div class="aegis-muted">SKU: {{ fields.sku }}</div>{% endif %}
      {% set sold_out = fields.stock is defined and fields.stock is not none and fields.stock | int <= 0 %}
      <div class="aegis-buy">
        {% if sold_out %}
        <span class="aegis-soldout">Sold out</span>
        {% elif theme_vars.shop_enabled == "true" %}
        <input id="aegis-qty" class="aegis-qty" type="number" min="1" value="1" aria-label="Quantity">
        <button type="button" class="aegis-button" data-aegis-add-to-cart data-quantity-input="#aegis-qty"
          data-product-id="{{ fields.id }}" data-product-slug="{{ slug }}" data-product-title="{{ title }}" data-product-price="{{ fields.price }}">Add to cart</button>
        {% elif fields.buy_url %}
        <a class="aegis-button" href="{{ fields.buy_url }}" rel="noopener">Buy now</a>
        {% endif %}
        {% if not sold_out and fields.stock is defined and fields.stock is not none and fields.stock | int <= 5 %}<span class="aegis-muted">Only {{ fields.stock }} left</span>{% endif %}
      </div>
      {% if fields.excerpt %}<p style="margin-bottom: 1rem;">{{ fields.excerpt }}</p>{% endif %}
      <div class="prose">{{ content_html | safe }}</div>
    </div>
  </article>
  {% if comments %}{% include "comments.html" %}{% endif %}
</section>
{% endblock %}
"##;

pub const CART_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<section class="aegis-shop">
  {% include "shop_common.html" %}
  <h1 style="font-size: 2rem; margin-bottom: 1.5rem;">{{ title }}</h1>
  <div id="aegis-cart"><p class="aegis-muted">Loading your cart…</p></div>
  <noscript><p class="aegis-error">The cart needs JavaScript.</p></noscript>
</section>
{% endblock %}
"#;

pub const ORDER_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<section class="aegis-shop" style="max-width: 720px;">
  {% include "shop_common.html" %}
  <h1 style="font-size: 2rem; margin-bottom: 1.5rem;">Thank you for your order</h1>
  <div id="aegis-order" data-order-id="{{ slug }}"><p class="aegis-muted">Loading your order…</p></div>
</section>
{% endblock %}
"#;

/// フォーラム共通スタイル・ナビゲーション
pub const FORUM_COMMON_TEMPLATE: &str = r##"<style>
  .aegis-forum { max-width: 1040px; margin: 0 auto; padding: 1rem 0 3rem; }
  .aegis-forum a:not(.aegis-forum-btn) { color: inherit; text-decoration: none; }
  .aegis-forum a:not(.aegis-forum-btn):hover { color: var(--accent, #6366f1); }
  .aegis-forum a.aegis-forum-btn { text-decoration: none; }
  .aegis-forum-reply-to { font-size: 0.82rem; font-weight: 600; opacity: 0.75; margin: 0 0 0.6rem; }
  .aegis-forum-nav { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 1rem; margin-bottom: 2rem; padding-bottom: 0.85rem; border-bottom: 1px solid rgba(127,127,127,0.22); font-size: 0.92rem; }
  .aegis-forum-breadcrumbs { display: flex; align-items: center; gap: 0.5rem; opacity: 0.9; }
  .aegis-forum-breadcrumbs span { opacity: 0.5; }
  .aegis-forum-card { background: rgba(127,127,127,0.06); border: 1px solid rgba(127,127,127,0.18); border-radius: 14px; padding: 1.25rem 1.5rem; margin-bottom: 1rem; transition: transform 0.15s ease, border-color 0.15s ease; }
  .aegis-forum-card:hover { border-color: rgba(99,102,241,0.45); }
  .aegis-forum-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 1.25rem; margin-bottom: 2.5rem; }
  .aegis-forum-cat-title { font-size: 1.2rem; font-weight: 700; margin: 0 0 0.4rem; display: flex; align-items: center; justify-content: space-between; }
  .aegis-forum-cat-desc { font-size: 0.88rem; opacity: 0.75; margin: 0 0 1rem; line-height: 1.5; }
  .aegis-forum-cat-stats { font-size: 0.8rem; opacity: 0.65; display: flex; gap: 1.2rem; }
  .aegis-forum-threads { display: flex; flex-direction: column; gap: 0.75rem; margin-bottom: 2rem; }
  .aegis-forum-thread-item { display: flex; align-items: center; justify-content: space-between; gap: 1rem; padding: 1rem 1.25rem; background: rgba(127,127,127,0.05); border: 1px solid rgba(127,127,127,0.16); border-radius: 12px; transition: all 0.15s ease; }
  .aegis-forum-thread-item:hover { background: rgba(127,127,127,0.08); border-color: rgba(99,102,241,0.4); transform: translateX(3px); }
  .aegis-forum-thread-main { flex: 1; min-width: 0; }
  .aegis-forum-thread-title { font-size: 1.05rem; font-weight: 600; margin: 0 0 0.35rem; display: flex; align-items: center; gap: 0.5rem; overflow-wrap: anywhere; }
  .aegis-forum-thread-meta { font-size: 0.82rem; opacity: 0.65; display: flex; flex-wrap: wrap; gap: 0.8rem; align-items: center; }
  .aegis-forum-badge { display: inline-flex; align-items: center; gap: 0.25rem; padding: 0.15rem 0.5rem; border-radius: 999px; font-size: 0.72rem; font-weight: 600; line-height: 1; }
  .aegis-forum-badge-pinned { background: rgba(245,158,11,0.18); color: #f59e0b; border: 1px solid rgba(245,158,11,0.35); }
  .aegis-forum-badge-locked { background: rgba(244,63,94,0.18); color: #f43f5e; border: 1px solid rgba(244,63,94,0.35); }
  .aegis-forum-badge-replies { background: rgba(99,102,241,0.16); color: var(--accent, #6366f1); font-weight: 700; border-radius: 8px; padding: 0.35rem 0.65rem; font-size: 0.85rem; text-align: center; min-width: 50px; }
  .aegis-forum-posts { display: flex; flex-direction: column; gap: 1.25rem; margin-bottom: 2.5rem; }
  .aegis-forum-post { background: rgba(127,127,127,0.05); border: 1px solid rgba(127,127,127,0.18); border-radius: 14px; padding: 1.25rem 1.5rem; position: relative; }
  .aegis-forum-post:target { border-color: var(--accent, #6366f1); box-shadow: 0 0 0 2px rgba(99,102,241,0.25); }
  .aegis-forum-post-head { display: flex; flex-wrap: wrap; justify-content: space-between; align-items: center; gap: 0.5rem; margin-bottom: 0.85rem; padding-bottom: 0.6rem; border-bottom: 1px solid rgba(127,127,127,0.15); font-size: 0.85rem; }
  .aegis-forum-post-author { display: flex; align-items: center; gap: 0.6rem; font-weight: 600; }
  .aegis-forum-avatar { width: 28px; height: 28px; border-radius: 50%; background: var(--accent, #6366f1); color: #fff; display: flex; align-items: center; justify-content: center; font-size: 0.75rem; font-weight: 700; }
  .aegis-forum-post-meta { opacity: 0.6; display: flex; align-items: center; gap: 0.8rem; font-size: 0.8rem; }
  .aegis-forum-post-body { line-height: 1.65; overflow-wrap: anywhere; }
  .aegis-forum-post-body p { margin: 0 0 0.8rem; }
  .aegis-forum-post-body p:last-child { margin-bottom: 0; }
  .aegis-forum-form { background: rgba(127,127,127,0.05); border: 1px solid rgba(127,127,127,0.2); border-radius: 14px; padding: 1.5rem; margin-top: 2rem; }
  .aegis-forum-form h3 { font-size: 1.25rem; margin: 0 0 1.25rem; font-weight: 700; }
  .aegis-forum-form-group { display: flex; flex-direction: column; gap: 0.4rem; margin-bottom: 1rem; }
  .aegis-forum-form-group label { font-size: 0.85rem; font-weight: 600; opacity: 0.85; }
  .aegis-forum-form input[type=text], .aegis-forum-form textarea { width: 100%; box-sizing: border-box; padding: 0.7rem 0.9rem; border-radius: 10px; border: 1px solid rgba(127,127,127,0.3); background: rgba(127,127,127,0.06); color: inherit; font: inherit; font-size: 0.92rem; }
  .aegis-forum-form input[type=text]:focus, .aegis-forum-form textarea:focus { outline: none; border-color: var(--accent, #6366f1); background: rgba(127,127,127,0.09); }
  .aegis-forum-btn { display: inline-flex; align-items: center; justify-content: center; gap: 0.4rem; padding: 0.65rem 1.4rem; border-radius: 10px; border: none; background: var(--accent, #6366f1); color: #fff; font: inherit; font-weight: 600; font-size: 0.9rem; cursor: pointer; text-decoration: none; transition: opacity 0.15s ease, transform 0.15s ease; }
  .aegis-forum-btn:hover { opacity: 0.92; transform: translateY(-1px); color: #fff; }
  .aegis-forum-btn-secondary { background: rgba(127,127,127,0.18); color: inherit; }
  .aegis-forum-btn-secondary:hover { background: rgba(127,127,127,0.28); color: inherit; }
  .aegis-forum-banner-locked { padding: 1rem 1.25rem; border-radius: 12px; background: rgba(244,63,94,0.12); border: 1px solid rgba(244,63,94,0.3); color: #f43f5e; font-weight: 600; font-size: 0.92rem; margin-top: 1.5rem; text-align: center; }
  .aegis-forum-banner-notice { padding: 0.85rem 1.25rem; border-radius: 12px; background: rgba(16,185,129,0.12); border: 1px solid rgba(16,185,129,0.3); color: #10b981; font-weight: 600; font-size: 0.9rem; margin-bottom: 1.5rem; }
  .aegis-forum-report { margin-top: 0.75rem; font-size: 0.8rem; }
  .aegis-forum-report summary { cursor: pointer; opacity: 0.55; list-style: none; display: inline-block; }
  .aegis-forum-report summary:hover { opacity: 0.9; }
  .aegis-forum-report-form { display: flex; flex-wrap: wrap; gap: 0.5rem; margin-top: 0.5rem; align-items: center; }
  .aegis-forum-report-form input[type=text] { flex: 1; min-width: 180px; padding: 0.4rem 0.65rem; border-radius: 8px; border: 1px solid rgba(127,127,127,0.3); background: rgba(127,127,127,0.06); color: inherit; font: inherit; font-size: 0.82rem; }
  .aegis-forum-empty { padding: 3rem 1rem; text-align: center; opacity: 0.6; font-size: 0.95rem; }
</style>
"##;

/// フォーラムトップ画面（カテゴリ一覧 & 最近のスレッド）
pub const FORUM_INDEX_TEMPLATE: &str = r##"{% extends "base.html" %}

{% block content %}
{% include "forum_common.html" %}
<section class="aegis-forum">
  <nav class="aegis-forum-nav" aria-label="Forum navigation">
    <div class="aegis-forum-breadcrumbs">
      <strong>Community Forum</strong>
    </div>
  </nav>

  <h1 style="font-size: 2.2rem; margin: 0 0 0.5rem; font-weight: 800; letter-spacing: -0.02em;">Community Forum</h1>
  <p style="opacity: 0.7; margin: 0 0 2.5rem; font-size: 1.05rem;">Join discussions, ask questions, and share knowledge with fellow members.</p>

  <h2 style="font-size: 1.35rem; font-weight: 700; margin: 0 0 1.25rem;">Categories</h2>
  <div class="aegis-forum-grid">
    {% for cat in fields.categories %}
    <a href="{{ theme_vars.forum_url }}/{{ cat.slug }}" class="aegis-forum-card">
      <div class="aegis-forum-cat-title">
        <span>{{ cat.name }}</span>
        <span style="opacity: 0.5; font-size: 0.9rem;">&rarr;</span>
      </div>
      <p class="aegis-forum-cat-desc">{{ cat.description }}</p>
      <div class="aegis-forum-cat-stats">
        <span><strong>{{ cat.thread_count }}</strong> threads</span>
        <span><strong>{{ cat.post_count }}</strong> posts</span>
      </div>
    </a>
    {% else %}
    <p class="aegis-forum-empty">No categories available.</p>
    {% endfor %}
  </div>

  {% if fields.recent_threads | length > 0 %}
  <h2 style="font-size: 1.35rem; font-weight: 700; margin: 2rem 0 1.25rem;">Recent Discussions</h2>
  <div class="aegis-forum-threads">
    {% for t in fields.recent_threads %}
    <a href="{{ theme_vars.forum_url }}/{{ t.category_slug }}/{{ t.id }}" class="aegis-forum-thread-item">
      <div class="aegis-forum-thread-main">
        <div class="aegis-forum-thread-title">
          {% if t.is_pinned %}<span class="aegis-forum-badge aegis-forum-badge-pinned">PIN</span>{% endif %}
          {% if t.is_locked %}<span class="aegis-forum-badge aegis-forum-badge-locked">LOCKED</span>{% endif %}
          <span>{{ t.title }}</span>
        </div>
        <div class="aegis-forum-thread-meta">
          <span>in <strong>{{ t.category_name }}</strong></span>
          <span>by {{ t.author_name }}</span>
          <span>Last active <time datetime="{{ t.last_post_at }}">{{ t.last_post_at | datetime_short }}</time></span>
        </div>
      </div>
      <div class="aegis-forum-badge-replies" title="Total replies">
        {{ t.post_count }}
      </div>
    </a>
    {% endfor %}
  </div>
  {% endif %}
</section>
{% endblock %}
"##;

/// カテゴリ別スレッド一覧
pub const FORUM_CATEGORY_TEMPLATE: &str = r##"{% extends "base.html" %}

{% block content %}
{% include "forum_common.html" %}
<section class="aegis-forum">
  <nav class="aegis-forum-nav" aria-label="Forum navigation">
    <div class="aegis-forum-breadcrumbs">
      <a href="{{ theme_vars.forum_url }}">Forum</a>
      <span>&gt;</span>
      <strong>{{ fields.category.name }}</strong>
    </div>
    <a href="#new-thread" class="aegis-forum-btn">+ New Thread</a>
  </nav>

  <h1 style="font-size: 2rem; margin: 0 0 0.5rem; font-weight: 800;">{{ fields.category.name }}</h1>
  {% if fields.category.description %}
  <p style="opacity: 0.7; margin: 0 0 2rem; font-size: 1rem;">{{ fields.category.description }}</p>
  {% endif %}
  {% if fields.pending_notice %}
  <div class="aegis-forum-banner-notice" role="status">Your thread was submitted and is awaiting moderator approval.</div>
  {% endif %}

  <div class="aegis-forum-threads">
    {% for t in fields.threads %}
    <a href="{{ theme_vars.forum_url }}/{{ fields.category.slug }}/{{ t.id }}" class="aegis-forum-thread-item">
      <div class="aegis-forum-thread-main">
        <div class="aegis-forum-thread-title">
          {% if t.is_pinned %}<span class="aegis-forum-badge aegis-forum-badge-pinned">PIN</span>{% endif %}
          {% if t.is_locked %}<span class="aegis-forum-badge aegis-forum-badge-locked">LOCKED</span>{% endif %}
          <span>{{ t.title }}</span>
        </div>
        <div class="aegis-forum-thread-meta">
          <span>Started by <strong>{{ t.author_name }}</strong></span>
          <span>Last post <time datetime="{{ t.last_post_at }}">{{ t.last_post_at | datetime_short }}</time></span>
        </div>
      </div>
      <div class="aegis-forum-badge-replies" title="Total replies">
        {{ t.post_count }}
      </div>
    </a>
    {% else %}
    <div class="aegis-forum-card aegis-forum-empty">
      No threads in this category yet. Be the first to start a conversation!
    </div>
    {% endfor %}
  </div>

  {% if pagination %}
  <div style="display: flex; justify-content: center; gap: 0.5rem; margin: 2rem 0;">
    {% if pagination.prev_url %}
    <a href="{{ pagination.prev_url }}" class="aegis-forum-btn aegis-forum-btn-secondary">&larr; Previous</a>
    {% endif %}
    {% if pagination.next_url %}
    <a href="{{ pagination.next_url }}" class="aegis-forum-btn aegis-forum-btn-secondary">Next &rarr;</a>
    {% endif %}
  </div>
  {% endif %}

  <section class="aegis-forum-form" id="new-thread">
    <h3>Start a New Thread</h3>
    <form method="POST" action="{{ theme_vars.forum_url }}/{{ fields.category.slug }}/threads">
      <div class="aegis-forum-form-group">
        <label for="thread-title">Thread Title</label>
        <input type="text" id="thread-title" name="title" required minlength="2" maxlength="200" placeholder="Summarize your question or topic...">
      </div>
      <div class="aegis-forum-form-group">
        <label for="thread-author">Your Name / Handle</label>
        <input type="text" id="thread-author" name="author_name" placeholder="Anonymous" maxlength="50">
      </div>
      <div class="aegis-forum-form-group">
        <label for="thread-body">Message Body</label>
        <textarea id="thread-body" name="body" rows="6" required minlength="1" placeholder="Write your post here..."></textarea>
      </div>
      <button type="submit" class="aegis-forum-btn">Create Thread</button>
    </form>
  </section>
</section>
{% endblock %}
"##;

/// スレッド詳細 & レス一覧画面
pub const FORUM_THREAD_TEMPLATE: &str = r##"{% extends "base.html" %}

{% block content %}
{% include "forum_common.html" %}
<section class="aegis-forum">
  <nav class="aegis-forum-nav" aria-label="Forum navigation">
    <div class="aegis-forum-breadcrumbs">
      <a href="{{ theme_vars.forum_url }}">Forum</a>
      <span>&gt;</span>
      <a href="{{ theme_vars.forum_url }}/{{ fields.category.slug }}">{{ fields.category.name }}</a>
      <span>&gt;</span>
      <strong style="max-width: 300px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">{{ fields.thread.title }}</strong>
    </div>
    {% if not fields.thread.is_locked %}
    <a href="#reply-form" class="aegis-forum-btn">Reply</a>
    {% endif %}
  </nav>

  <header style="margin-bottom: 2rem;">
    <div style="display: flex; align-items: center; gap: 0.6rem; margin-bottom: 0.5rem; flex-wrap: wrap;">
      {% if fields.thread.is_pinned %}<span class="aegis-forum-badge aegis-forum-badge-pinned">PINNED</span>{% endif %}
      {% if fields.thread.is_locked %}<span class="aegis-forum-badge aegis-forum-badge-locked">LOCKED</span>{% endif %}
      <h1 style="font-size: 1.85rem; margin: 0; font-weight: 800; line-height: 1.3;">{{ fields.thread.title }}</h1>
    </div>
    <div style="font-size: 0.85rem; opacity: 0.65; display: flex; gap: 1rem; flex-wrap: wrap;">
      <span>Started by <strong>{{ fields.thread.author_name }}</strong></span>
      <span>{{ fields.thread.post_count }} posts</span>
      <span>Created <time datetime="{{ fields.thread.created_at }}">{{ fields.thread.created_at | date_short }}</time></span>
    </div>
  </header>

  {% if fields.pending_notice %}
  <div class="aegis-forum-banner-notice" role="status">Your reply was submitted and is awaiting moderator approval.</div>
  {% endif %}
  {% if fields.reported_notice %}
  <div class="aegis-forum-banner-notice" role="status">Thank you. Your report has been submitted to the moderators.</div>
  {% endif %}

  <div class="aegis-forum-posts">
    {% for p in fields.posts %}
    <article class="aegis-forum-post" id="post-{{ p.post_number }}">
      <div class="aegis-forum-post-head">
        <div class="aegis-forum-post-author">
          <div class="aegis-forum-avatar">{{ p.author_initial }}</div>
          <span>{{ p.author_name }}</span>
          {% if p.is_author_staff %}
          <span class="aegis-forum-badge" style="background: var(--accent, #6366f1); color: #fff;">Staff</span>
          {% endif %}
        </div>
        <div class="aegis-forum-post-meta">
          <time datetime="{{ p.created_at }}">{{ p.created_at | datetime_short }}</time>
          <a href="#post-{{ p.post_number }}" style="font-weight: 700; opacity: 0.8;">#{{ p.post_number }}</a>
        </div>
      </div>
      {% if p.reply_to_number %}
      <p class="aegis-forum-reply-to"><a href="#post-{{ p.reply_to_number }}">&gt;&gt;{{ p.reply_to_number }}</a></p>
      {% endif %}
      <div class="aegis-forum-post-body">
        {{ p.body_html | safe }}
      </div>
      <details class="aegis-forum-report">
        <summary>Report</summary>
        <form class="aegis-forum-report-form" method="POST" action="{{ theme_vars.forum_url }}/{{ fields.category.slug }}/{{ fields.thread.id }}/report">
          <input type="hidden" name="target_type" value="post">
          <input type="hidden" name="target_id" value="{{ p.id }}">
          <input type="text" name="reason" required maxlength="500" placeholder="Reason for reporting this post...">
          <button type="submit" class="aegis-forum-btn aegis-forum-btn-secondary" style="padding: 0.4rem 0.8rem; font-size: 0.8rem;">Report</button>
        </form>
      </details>
    </article>
    {% endfor %}
  </div>

  {% if pagination %}
  <div style="display: flex; justify-content: center; gap: 0.5rem; margin: 2rem 0;">
    {% if pagination.prev_url %}
    <a href="{{ pagination.prev_url }}" class="aegis-forum-btn aegis-forum-btn-secondary">&larr; Previous</a>
    {% endif %}
    {% if pagination.next_url %}
    <a href="{{ pagination.next_url }}" class="aegis-forum-btn aegis-forum-btn-secondary">Next &rarr;</a>
    {% endif %}
  </div>
  {% endif %}

  {% if fields.thread.is_locked %}
  <div class="aegis-forum-banner-locked">
    🔒 This thread has been locked. New replies are not accepted.
  </div>
  {% else %}
  <section class="aegis-forum-form" id="reply-form">
    <h3>Post a Reply</h3>
    <form method="POST" action="{{ theme_vars.forum_url }}/{{ fields.category.slug }}/{{ fields.thread.id }}/posts">
      <div class="aegis-forum-form-group">
        <label for="post-author">Your Name / Handle</label>
        <input type="text" id="post-author" name="author_name" placeholder="Anonymous" maxlength="50">
      </div>
      <div class="aegis-forum-form-group">
        <label for="post-body">Reply Content</label>
        <textarea id="post-body" name="body" rows="5" required minlength="1" placeholder="Write your response..."></textarea>
      </div>
      <button type="submit" class="aegis-forum-btn">Submit Reply</button>
    </form>
  </section>
  {% endif %}
</section>
{% endblock %}
"##;
