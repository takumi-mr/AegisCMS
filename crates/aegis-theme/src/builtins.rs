pub const BASE_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="{{ locale }}">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{% if title %}{{ title }} | {% endif %}{{ site_title }}</title>
  <meta name="description" content="{% if site_description %}{{ site_description }}{% else %}{{ site_title }}{% endif %}">
  {% filter inline_style %}
    /* Fonts served by this site (/_aegis/fonts/): no request to a font CDN, so nothing for the CSP to block.
       Only the Latin subset; other scripts use the system font (Japanese text falls back to the system font) */
    @font-face { font-family: 'Plus Jakarta Sans'; font-style: normal; font-weight: 400 800; font-display: swap; src: url('{{ base_path }}/_aegis/fonts/plus-jakarta-sans-latin.woff2') format('woff2'); unicode-range: U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD; }
    @font-face { font-family: 'JetBrains Mono'; font-style: normal; font-weight: 400 500; font-display: swap; src: url('{{ base_path }}/_aegis/fonts/jetbrains-mono-latin.woff2') format('woff2'); unicode-range: U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD; }
    html { scroll-behavior: smooth; }
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
  {% endfilter %}
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
        <a href="{{ base_path }}/" class="nav-link">{{ t("nav.home") }}</a>
        {% if admin_url %}<a href="{{ admin_url }}" class="admin-pill">{{ t("admin.studio") }}</a>{% endif %}
      </nav>
    </div>
  </header>

  <main class="site-main">
    {% block content %}{% endblock %}
  </main>

  <footer class="site-footer">
    <div class="footer-content">
      {% if show_powered_by %}
      <div class="footer-badge">
        <span>Powered by <strong>AegisCMS</strong></span>
        <span>•</span>
        <span>Zero-Trust WebAssembly Architecture</span>
      </div>
      {% endif %}
      <div>&copy; 2026 {{ site_title }}. {{ t("footer.rights") }}</div>
    </div>
  </footer>
</body>
</html>
"#;

pub const INDEX_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<section class="hero-section aegis-s1">
  {% if show_powered_by %}
  <div class="aegis-s2">
    {{ t("index.badge") }}
  </div>
  {% endif %}
  <h1 class="aegis-s3">
    {{ site_title }}
  </h1>
  <p class="aegis-s4">
    {{ site_description }}
  </p>
</section>

<section class="articles-section">
  <div class="aegis-s5">
    <h2 class="aegis-s6">{{ t("index.latest") }}</h2>
    <span class="aegis-s7">{{ t("index.posts", n=(pagination.total_items if pagination else items | length)) }}</span>
  </div>

  {% if items and items | length > 0 %}
  <div class="cards-grid aegis-s8">
    {% for item in items %}
    <article class="article-card aegis-s9">
      <div>
        <div class="aegis-s10">
          <span class="aegis-s11">
            {{ item.content_type }}
          </span>
          {% if item.published_at %}
          <time class="aegis-s12" datetime="{{ item.published_at }}">
            {{ item.published_at | truncate(10, True, "") }}
          </time>
          {% endif %}
        </div>
        <h3 class="aegis-s13">
          <a class="aegis-s14" href="{{ base_path }}/articles/{{ item.slug }}">
            {{ item.title }}
          </a>
        </h3>
        {% if item.excerpt %}
        <p class="aegis-s15">
          {{ item.excerpt }}
        </p>
        {% endif %}
      </div>
      <div class="aegis-s16">
        <span class="aegis-s12">
          {% if item.author_id %}{{ t("author.by", name=item.author_id) }}{% else %}{{ t("author.team") }}{% endif %}
        </span>
        <a class="aegis-s17" href="{{ base_path }}/articles/{{ item.slug }}">
          {{ t("read_story") }} &rarr;
        </a>
      </div>
    </article>
    {% endfor %}
  </div>
  {% else %}
  <div class="aegis-s18">
    <div class="aegis-s19">
      &bull;
    </div>
    <h3 class="aegis-s20">{{ t("empty.title") }}</h3>
    {% if admin_url %}<p class="aegis-s21">{{ t("empty.hint") }}</p>{% endif %}
    {% if admin_url %}<a href="{{ admin_url }}" class="admin-pill aegis-s22">{{ t("empty.launch") }}</a>{% endif %}
  </div>
  {% endif %}
{% if pagination and pagination.total_pages > 1 %}
<nav class="pagination aegis-s23" aria-label="{{ t("pagination.label") }}">
  {% if pagination.prev_url %}<a href="{{ pagination.prev_url }}" rel="prev" class="admin-pill">&larr; {{ t("pagination.newer_stories") }}</a>{% else %}<span></span>{% endif %}
  <span>{{ t("pagination.page", page=pagination.page, total=pagination.total_pages) }}</span>
  {% if pagination.next_url %}<a href="{{ pagination.next_url }}" rel="next" class="admin-pill">{{ t("pagination.older_stories") }} &rarr;</a>{% else %}<span></span>{% endif %}
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
:where(.aegis-access-notice){border:1px solid rgba(127,127,127,.3);border-radius:8px;padding:1rem 1.25rem;margin:1.5rem 0}\
:where(.aegis-access-notice) p{margin:.25rem 0}\
:where(.aegis-fnref){font-size:.75em;line-height:0}\
:where(.aegis-footnotes){margin-top:2rem;padding-top:.75rem;border-top:1px solid rgba(127,127,127,.3);font-size:.9em}\
:where(.aegis-footnotes) ol{margin:0;padding-left:1.5rem}\
:where(.aegis-footnotes) p{margin:.25rem 0}\
:where(.aegis-fn-backref){text-decoration:none;margin-left:.15em}\
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
:where(.aegis-linkcard){margin:1rem 0}\
:where(.aegis-linkcard)>a{display:flex;flex-direction:column;gap:.15rem;padding:.7rem 1rem;border:1px solid rgba(127,127,127,.3);border-radius:10px;color:inherit;text-decoration:none;overflow:hidden}\
:where(.aegis-linkcard)>a:hover{border-color:rgba(127,127,127,.7)}\
:where(.aegis-linkcard-site){font-weight:600}\
:where(.aegis-linkcard-path){opacity:.65;font-size:.85em;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}\
:where(p) img{max-width:100%;vertical-align:middle}\
:where(.aegis-quote){margin:1.5rem 0}\
:where(.aegis-quote) figcaption{opacity:.7;font-size:.9em;margin-top:.25rem}\
:where(.aegis-contact-form){display:flex;flex-direction:column;gap:1rem;margin:1.5rem 0;max-width:40rem}\
:where(.aegis-contact-field){display:flex;flex-direction:column;gap:.35rem}\
:where(.aegis-contact-field) label{font-weight:600;font-size:.95em}\
:where(.aegis-contact-checkbox) label{font-weight:400;display:flex;gap:.5rem;align-items:baseline}\
:where(.aegis-contact-field) :is(input:not([type=checkbox]),textarea,select){font:inherit;color:inherit;background:transparent;border:1px solid rgba(127,127,127,.45);border-radius:6px;padding:.55rem .7rem;width:100%;box-sizing:border-box}\
:where(.aegis-contact-field) :is(input,textarea,select)[aria-invalid=true]{border-color:#ef4444}\
:where(.aegis-contact-field) textarea{resize:vertical;min-height:8rem}\
:where(.aegis-contact-required){color:#ef4444}\
:where(.aegis-contact-error){color:#ef4444;font-size:.875em;margin:0}\
:where(.aegis-contact-description){margin:0;opacity:.8}\
:where(.aegis-contact-submit){align-self:flex-start;font:inherit;font-weight:600;padding:.6rem 1.4rem;border-radius:6px;border:1px solid rgba(127,127,127,.45);background:rgba(127,127,127,.12);color:inherit;cursor:pointer}\
:where(.aegis-contact-notice){border-left:3px solid #ef4444;padding:.5rem .9rem;margin:1rem 0}\
:where(.aegis-contact-sent){border-left:3px solid #10b981;padding:.75rem 1rem;margin:1.5rem 0}\
.aegis-ta-left{text-align:left}.aegis-ta-center{text-align:center}.aegis-ta-right{text-align:right}\
.aegis-contact-trap,.aegis-newsletter-trap{position:absolute;left:-10000px;width:1px;height:1px;overflow:hidden}\
:where(iframe.aegis-html-sandbox){width:100%;border:0;display:block;min-height:48px}\
</style>";

/// `[theme].front_page = "template"` のトップページ。テーマに `front.html` が無ければ記事一覧と同じ表示にする
pub const FRONT_TEMPLATE: &str = r#"{% extends "index.html" %}"#;

/// カテゴリの記事一覧（`/category/{slug}`）。どのテーマでも読めるよう、色は文字色の濃淡だけで付ける
pub const CATEGORY_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<section class="aegis-archive aegis-s24">
  <p class="aegis-archive-label aegis-s25">{{ t("category.label") }}</p>
  <h1 class="aegis-s26">{{ title }}</h1>
  {% if fields.description %}<p class="aegis-archive-description aegis-s27">{{ fields.description }}</p>{% endif %}
  <ul class="aegis-archive-list aegis-s28">
    {% for item in items %}
    <li class="aegis-s29">
      {% if item.published_at %}<time class="aegis-s30" datetime="{{ item.published_at }}">{{ item.published_at | date_short }}</time>{% endif %}
      <h2 class="aegis-s31"><a class="aegis-s32" href="{{ base_path }}/articles/{{ item.slug }}">{{ item.title }}</a></h2>
      {% if item.excerpt %}<p class="aegis-s33">{{ item.excerpt }}</p>{% endif %}
    </li>
    {% else %}
    <li class="aegis-s34">{{ t("category.empty") }}</li>
    {% endfor %}
  </ul>
  {% if pagination and pagination.total_pages > 1 %}
  <nav class="pagination aegis-s35" aria-label="{{ t("pagination.label") }}">
    {% if pagination.prev_url %}<a href="{{ pagination.prev_url }}" rel="prev">&larr; {{ t("pagination.newer") }}</a>{% else %}<span></span>{% endif %}
    <span class="aegis-s36">{{ pagination.page }} / {{ pagination.total_pages }}</span>
    {% if pagination.next_url %}<a href="{{ pagination.next_url }}" rel="next">{{ t("pagination.older") }} &rarr;</a>{% else %}<span></span>{% endif %}
  </nav>
  {% endif %}
  <p class="aegis-s37"><a href="{{ base_path }}/category/{{ slug }}/feed.xml">{{ t("category.feed") }}</a></p>
</section>
{% endblock %}
"#;

/// タグのページ（`/tags/{slug}`）。種類をまたいで、そのタグが付いた公開中のコンテンツを新しい順に並べる。
/// `title` がタグの名前、`fields.description` が説明、`items` の `fields.url` が公開ページの URL
pub const TAG_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<section class="aegis-archive aegis-s24">
  <p class="aegis-archive-label aegis-s25">{{ t("tag.label") }}</p>
  <h1 class="aegis-s26">{{ title }}</h1>
  {% if fields.description %}<p class="aegis-archive-description aegis-s27">{{ fields.description }}</p>{% endif %}
  <ul class="aegis-archive-list aegis-s28">
    {% for item in items %}
    <li class="aegis-s29">
      {% if item.published_at %}<time class="aegis-s30" datetime="{{ item.published_at }}">{{ item.published_at | date_short }}</time>{% endif %}
      <h2 class="aegis-s31"><a class="aegis-s32" href="{{ item.fields.url }}">{{ item.title }}</a></h2>
      {% if item.excerpt %}<p class="aegis-s33">{{ item.excerpt }}</p>{% endif %}
    </li>
    {% else %}
    <li class="aegis-s34">{{ t("tag.empty") }}</li>
    {% endfor %}
  </ul>
  {% if pagination and pagination.total_pages > 1 %}
  <nav class="pagination aegis-s35" aria-label="{{ t("pagination.label") }}">
    {% if pagination.prev_url %}<a href="{{ pagination.prev_url }}" rel="prev">&larr; {{ t("pagination.newer") }}</a>{% else %}<span></span>{% endif %}
    <span class="aegis-s36">{{ pagination.page }} / {{ pagination.total_pages }}</span>
    {% if pagination.next_url %}<a href="{{ pagination.next_url }}" rel="next">{{ t("pagination.older") }} &rarr;</a>{% else %}<span></span>{% endif %}
  </nav>
  {% endif %}
  <p class="aegis-s37"><a href="{{ base_path }}/tags/{{ slug }}/feed.xml">{{ t("tag.feed") }}</a></p>
</section>
{% endblock %}
"#;

/// タグの一覧（`/tags`）。公開中のコンテンツに付いているタグを名前順に並べる。`items` はタグで、`title` が名前、
/// `slug` が slug、`fields.url` がタグのページの URL、`fields.count` が公開中のコンテンツの数、`fields.description` が説明
pub const TAGS_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<section class="aegis-archive aegis-s24">
  <h1 class="aegis-s38">{{ t("tags.title") }}</h1>
  <ul class="aegis-tag-list aegis-s28">
    {% for item in items %}
    <li class="aegis-s39">
      <a class="aegis-s40" href="{{ item.fields.url }}">{{ item.title }}</a>
      <span class="aegis-s41"> &middot; {{ t("tags.count", n=item.fields.count) }}</span>
      {% if item.fields.description %}<p class="aegis-s42">{{ item.fields.description }}</p>{% endif %}
    </li>
    {% else %}
    <li class="aegis-s34">{{ t("tags.empty") }}</li>
    {% endfor %}
  </ul>
</section>
{% endblock %}
"#;

/// 問い合わせフォーム（`/contact`・`/contact/{name}`）。`fields.form_html` が組み込みのフォーム（エスケープ済み）、
/// `fields.sent` が送信した後、`fields.form` がフォームの設定（`title`・`fields`・`action`・`success_message` など）、
/// `fields.errors` が入力の誤り、`fields.notice` が送信できなかったときの知らせ（`[contact].locale` の言語）
pub const CONTACT_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<section class="aegis-contact-page aegis-s43">
  <h1 class="aegis-s44">{{ title }}</h1>
  {% if fields.sent %}
  <div class="aegis-contact-sent" role="status"><p class="aegis-s45">{{ fields.form.success_message }}</p></div>
  {% else %}
  {% if fields.notice %}<p class="aegis-contact-notice" role="alert">{{ fields.notice }}</p>{% endif %}
  <div class="aegis-contact">{{ fields.form_html | safe }}</div>
  {% endif %}
</section>
{% endblock %}
"#;

/// お知らせバナー（Admin Studio の Settings › Notices）。各ページの `<body>` の直後に差し込む断片で、`base.html` は継承しない。
/// テーマは `templates/notices.html` で丸ごと差し替えられる。値は `fields.notices`（今表示するお知らせ。`id`・`message`・
/// `link_url`・`link_label`・`style`（`info` / `success` / `warning` / `alert`）・`dismissible`・`rev`）と
/// `fields.preview`（テーマ開発サーバーのプレビュー）。スクリプト（`/_aegis/notices.js`）は、閉じられるお知らせの
/// `data-aegis-notice`（お知らせの ID）・`data-aegis-notice-rev`（版）と、その中の `data-aegis-notice-close`
/// （閉じるボタン。最初は `hidden`。スクリプトが使えるときだけ表示する）で動く
pub const NOTICES_TEMPLATE: &str = r#"<div class="aegis-notices" role="region" aria-label="{{ t("notice.label") }}">
{% for notice in fields.notices %}
  <div class="aegis-notice aegis-notice-{{ notice.style }}" data-aegis-notice="{{ notice.id }}" data-aegis-notice-rev="{{ notice.rev }}"{% if notice.style == "warning" or notice.style == "alert" %} role="alert"{% else %} role="status"{% endif %}>
    <p class="aegis-notice-message">{{ notice.message }}{% if notice.link_url %} <a class="aegis-notice-link" href="{{ notice.link_url }}">{% if notice.link_label %}{{ notice.link_label }}{% else %}{{ t("notice.more") }}{% endif %}</a>{% endif %}</p>
    {% if notice.dismissible %}<button type="button" class="aegis-notice-close" data-aegis-notice-close hidden aria-label="{{ t("notice.close") }}">&times;</button>{% endif %}
  </div>
{% endfor %}
</div>
"#;

/// Cookie の同意バナー（`[consent]`）。ページの `</body>` の前に差し込む断片で、`base.html` は継承しない。
/// テーマは `templates/consent.html` で丸ごと差し替えられる。スクリプト（`/_aegis/consent.js`）は `data-aegis-consent-*`
/// の属性で要素を見つけて動かす: `data-aegis-consent-root`（ダイアログ。最初は `hidden`）、
/// `data-aegis-consent-action="accept-all|reject-all|customize|save"`、`data-aegis-consent-details`（「選択する」で開く部分。
/// 最初は `hidden`）、`data-aegis-consent-category="<区分の ID>"`（チェックボックス）、`data-aegis-consent-reopen`
/// （選択したあとに出す設定のボタン。最初は `hidden`）。値は `fields`:
/// `consent`（`layout`・`position`・`backdrop`・`policy_url`・`reopen_button`・`delay_ms`・`cookie_days`）、
/// `message`（`[consent].message`。空なら既定の文）、`categories`（`id`・`label`・`description`。空なら既定の名前と説明）、
/// `preview`（テーマ開発サーバーのプレビュー。`true` なら最初から表示する）
pub const CONSENT_TEMPLATE: &str = r#"<div class="aegis-consent aegis-consent-layout-{{ fields.consent.layout }} aegis-consent-pos-{{ fields.consent.position }}{% if fields.consent.backdrop %} aegis-consent-backdrop{% endif %}" data-aegis-consent-root role="dialog"{% if fields.consent.layout == "modal" %} aria-modal="true"{% endif %} aria-labelledby="aegis-consent-title" aria-describedby="aegis-consent-message"{% if not fields.preview %} hidden{% endif %}>
  <div class="aegis-consent-body">
    <h2 class="aegis-consent-title" id="aegis-consent-title">{{ t("consent.title") }}</h2>
    <p class="aegis-consent-message" id="aegis-consent-message">{% if fields.message %}{{ fields.message }}{% else %}{{ t("consent.message") }}{% endif %}{% if fields.consent.policy_url %} <a class="aegis-consent-policy" href="{{ fields.consent.policy_url }}">{{ t("consent.policy") }}</a>{% endif %}</p>
    <div class="aegis-consent-details" data-aegis-consent-details hidden>
      <label class="aegis-consent-category"><input type="checkbox" checked disabled><span><strong>{{ t("consent.necessary") }}</strong><small>{{ t("consent.necessary_description") }}</small></span></label>
      {% for category in fields.categories %}
      <label class="aegis-consent-category"><input type="checkbox" data-aegis-consent-category="{{ category.id }}"><span><strong>{% if category.label %}{{ category.label }}{% else %}{{ t("consent.category." ~ category.id) }}{% endif %}</strong><small>{% if category.description %}{{ category.description }}{% else %}{{ t("consent.category." ~ category.id ~ ".description") }}{% endif %}</small></span></label>
      {% endfor %}
    </div>
    <div class="aegis-consent-actions">
      <button type="button" class="aegis-consent-button" data-aegis-consent-action="reject-all">{{ t("consent.reject_all") }}</button>
      <button type="button" class="aegis-consent-button" data-aegis-consent-action="customize">{{ t("consent.customize") }}</button>
      <button type="button" class="aegis-consent-button" data-aegis-consent-action="save" hidden>{{ t("consent.save") }}</button>
      <button type="button" class="aegis-consent-button" data-aegis-consent-action="accept-all">{{ t("consent.accept_all") }}</button>
    </div>
  </div>
</div>
{% if fields.consent.reopen_button %}<button type="button" class="aegis-consent-button aegis-consent-reopen" data-aegis-consent-reopen hidden>{{ t("consent.reopen") }}</button>{% endif %}
"#;

/// サイト内検索の結果（`/search?q=...`）。`fields.query` が検索語、`items` の `fields.url` が公開ページの URL、
/// `fields.snippet_html` が検索語を `<mark>` で囲んだ本文の抜粋（エスケープ済み）。
/// `fields.category`（絞り込み中のカテゴリ ID）と `fields.tag`（絞り込み中のタグ）、`fields.sort`（`relevance` / `newest`）も渡される
pub const SEARCH_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
{% filter inline_style %}:where(.aegis-search) mark{background:rgba(250,204,21,.3);color:inherit;border-radius:2px;padding:0 .1em}{% endfilter %}
<section class="aegis-search aegis-s24">
  <h1 class="aegis-s44">{{ title }}</h1>
  <form class="aegis-search-form aegis-s46" role="search" action="{{ base_path }}/search" method="get">
    <input class="aegis-s47" type="search" name="q" value="{{ fields.query }}" aria-label="{{ t("search.label") }}" placeholder="{{ t("search.label") }}" maxlength="200">
    {% if fields.category %}<input type="hidden" name="category" value="{{ fields.category }}">{% endif %}
    {% if fields.tag %}<input type="hidden" name="tag" value="{{ fields.tag }}">{% endif %}
    <select class="aegis-s48" name="sort" aria-label="{{ t("search.sort") }}">
      <option value="relevance"{% if fields.sort != "newest" %} selected{% endif %}>{{ t("search.sort_relevance") }}</option>
      <option value="newest"{% if fields.sort == "newest" %} selected{% endif %}>{{ t("search.sort_newest") }}</option>
    </select>
    <button class="aegis-s49" type="submit">{{ t("search.label") }}</button>
  </form>
  {% if fields.query %}
  <p class="aegis-search-count aegis-s50">{{ t("search.results", n=pagination.total_items) }}</p>
  <ul class="aegis-search-results aegis-s28">
    {% for item in items %}
    <li class="aegis-s29">
      <h2 class="aegis-s51"><a class="aegis-s32" href="{{ item.fields.url }}">{{ item.title }}</a></h2>
      {% if item.fields.snippet_html %}<p class="aegis-search-snippet aegis-s52">{{ item.fields.snippet_html | safe }}</p>{% endif %}
      {% if item.published_at %}<time class="aegis-s53" datetime="{{ item.published_at }}">{{ item.published_at | date_short }}</time>{% endif %}
    </li>
    {% else %}
    <li class="aegis-s34">{{ t("search.empty") }}</li>
    {% endfor %}
  </ul>
  {% if pagination and pagination.total_pages > 1 %}
  <nav class="pagination aegis-s35" aria-label="{{ t("pagination.label") }}">
    {% if pagination.prev_url %}<a href="{{ pagination.prev_url }}" rel="prev">&larr; {{ t("pagination.previous") }}</a>{% else %}<span></span>{% endif %}
    <span class="aegis-s36">{{ pagination.page }} / {{ pagination.total_pages }}</span>
    {% if pagination.next_url %}<a href="{{ pagination.next_url }}" rel="next">{{ t("pagination.next") }} &rarr;</a>{% else %}<span></span>{% endif %}
  </nav>
  {% endif %}
  {% endif %}
</section>
{% endblock %}
"#;

pub const ARTICLE_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<article class="article-detail aegis-s54">
  <nav class="breadcrumb aegis-s55">
    <a class="aegis-s56" href="{{ base_path }}/">{{ t("nav.home") }}</a>
    <span>/</span>
    <span class="aegis-s57">{{ content_type }}</span>
    <span>/</span>
    <span class="aegis-s58">{{ title }}</span>
  </nav>

  <header class="article-header aegis-s59">
    <div class="aegis-s60">
      <span class="aegis-s61">
        {{ content_type }}
      </span>
      {% if published_at %}
      <time class="aegis-s7" datetime="{{ published_at }}">
        {{ t("article.published_on", date=(published_at | date_short)) }}
      </time>
      {% endif %}
    </div>

    <h1 class="aegis-s62">
      {{ title }}
    </h1>

    <div class="aegis-s63">
      <div class="aegis-s64">
        {% if author %}{{ author | truncate(1, True, "") | upper }}{% else %}A{% endif %}
      </div>
      <div>
        <div class="aegis-s65">
          {% if author %}{{ author }}{% else %}{{ t("author.default") }}{% endif %}
        </div>
        <div class="aegis-s66">{{ t("author.verified") }}</div>
      </div>
    </div>
    {% if fields.tags %}<ul class="aegis-tags aegis-s67" aria-label="{{ t("tags.aria") }}">{% for tag in fields.tags %}<li><a class="aegis-s68" href="{{ tag.url }}" rel="tag">{{ tag.name }}</a></li>{% endfor %}</ul>{% endif %}
  </header>

  <div class="prose">
    {{ content_html | safe }}
  </div>

  {% if related_articles and related_articles | length > 0 %}
  <section class="related-articles aegis-s69">
    <h2 class="aegis-s70">
      {{ t("article.related") }}
    </h2>
    <div class="aegis-s71">
      {% for item in related_articles %}
      <a class="aegis-s72" href="{{ base_path }}/articles/{{ item.slug }}">
        <h3 class="aegis-s73">
          {{ item.title }}
        </h3>
        {% if item.excerpt %}
        <p class="aegis-s74">
          {{ item.excerpt }}
        </p>
        {% endif %}
        {% if item.published_at %}
        <time class="aegis-s75" datetime="{{ item.published_at }}">
          {{ item.published_at | date_short }}
        </time>
        {% endif %}
      </a>
      {% endfor %}
    </div>
  </section>
  {% endif %}

  {% if comments %}{% include "comments.html" %}{% endif %}

  <footer class="article-footer aegis-s76">
    <a class="aegis-s77" href="{{ base_path }}/">
      &larr; {{ t("article.back") }}
    </a>
    <a class="aegis-s78" href='#'>
      {{ t("article.scroll_top") }} &uarr;
    </a>
  </footer>
</article>
{% endblock %}
"#;

pub const PAGE_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<article class="page-detail aegis-s54">
  <header class="aegis-s79">
    <h1 class="aegis-s80">
      {{ title }}
    </h1>
  </header>

  {% if fields.tags %}<ul class="aegis-tags aegis-s81" aria-label="{{ t("tags.aria") }}">{% for tag in fields.tags %}<li><a class="aegis-s68" href="{{ tag.url }}" rel="tag">{{ tag.name }}</a></li>{% endfor %}</ul>{% endif %}
  <div class="prose">
    {{ content_html | safe }}
  </div>
  {% if related_articles and related_articles | length > 0 %}
  <section class="related-articles aegis-s69">
    <h2 class="aegis-s70">
      {{ t("article.related") }}
    </h2>
    <div class="aegis-s71">
      {% for item in related_articles %}
      <a class="aegis-s72" href="{{ base_path }}/articles/{{ item.slug }}">
        <h3 class="aegis-s73">
          {{ item.title }}
        </h3>
        {% if item.excerpt %}
        <p class="aegis-s74">
          {{ item.excerpt }}
        </p>
        {% endif %}
        {% if item.published_at %}
        <time class="aegis-s75" datetime="{{ item.published_at }}">
          {{ item.published_at | date_short }}
        </time>
        {% endif %}
      </a>
      {% endfor %}
    </div>
  </section>
  {% endif %}

  {% if comments %}{% include "comments.html" %}{% endif %}
</article>
{% endblock %}
"#;

pub const BLANK_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="{{ locale }}">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{% if title %}{{ title }} | {% endif %}{{ site_title }}</title>
  <meta name="description" content="{% if site_description %}{{ site_description }}{% else %}{{ site_title }}{% endif %}">
  {% filter inline_style %}
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
  {% endfilter %}
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
<div class="aegis-s82">
  <div class="aegis-s83">
    404
  </div>
  <h2 class="aegis-s84">{{ t("notfound.title") }}</h2>
  <p class="aegis-s85">
    {{ t("notfound.message") }}
  </p>
  <a class="aegis-s86" href="{{ base_path }}/">
    {{ t("notfound.home") }}
  </a>
</div>
{% endblock %}
"#;

/// ショップのページ共通のスタイル・ナビゲーション・カートのスクリプト（各ショップテンプレートに含める）。
/// どのテーマの `base.html` の中でも読めるよう、色はテーマの文字色を基準にした半透明で指定する
pub const SHOP_COMMON_TEMPLATE: &str = r#"{% filter inline_style %}
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
  .aegis-account-form { display: flex; flex-direction: column; gap: 0.9rem; max-width: 420px; margin: 1rem 0; }
  .aegis-account-form button { align-self: flex-start; }
  .aegis-account-list { list-style: none; padding: 0; display: flex; flex-direction: column; gap: 0.75rem; margin: 0.75rem 0 2rem; }
  .aegis-account-order, .aegis-account-review { padding: 0.9rem 1.1rem; border-radius: 14px; border: 1px solid rgba(127,127,127,0.25); display: flex; flex-direction: column; gap: 0.4rem; }
  .aegis-account-order-lines { display: flex; flex-direction: column; gap: 0.3rem; padding-top: 0.5rem; }
  .aegis-account-ok { color: #10b981; margin: 0.75rem 0; }
  .aegis-shop h2 { font-size: 1.3rem; margin: 1.5rem 0 0.75rem; }
{% endfilter %}
<nav class="aegis-shop-nav" aria-label="{{ t("shop.label") }}">
  <a href="{{ theme_vars.products_url }}">{{ t("shop.all_products") }}</a>
  {% if theme_vars.account_enabled == "true" %}<a href="{{ theme_vars.account_url }}" class="aegis-account-link">{{ t("account.link") }}</a>{% endif %}
  <a href="{{ theme_vars.cart_url }}" class="aegis-cart-link">{{ t("shop.cart") }} <span class="aegis-badge" data-aegis-cart-count hidden>0</span></a>
</nav>
<script src="{{ theme_vars.cart_script }}" data-api="{{ theme_vars.shop_api }}" data-currency="{{ theme_vars.currency }}" data-cart-url="{{ theme_vars.cart_url }}" data-products-url="{{ theme_vars.products_url }}" data-shop-enabled="{{ theme_vars.shop_enabled }}" defer></script>"#;

/// コメント欄（`[comments]`）。`article.html` / `page.html` / `product.html` が
/// `{% if comments %}{% include "comments.html" %}{% endif %}` で読み込む。どのテーマの中でも読めるよう、
/// 色はテーマの文字色を基準にした半透明で指定する。投稿はスクリプトで JSON を送る
pub const COMMENTS_TEMPLATE: &str = r##"<section class="aegis-comments" id="comments">
{% filter inline_style %}
  .aegis-comments { margin-top: 3rem; padding-top: 2rem; border-top: 1px solid rgba(127,127,127,0.25); }
  .aegis-comments h2 { font-size: 1.35rem; margin-bottom: 1.25rem; }
  .aegis-comments h3 { font-size: 1.1rem; margin-bottom: 0.75rem; }
  .aegis-comment { margin: 0 0 1rem 0; padding: 0.9rem 1rem; border-radius: 12px; background: rgba(127,127,127,0.07); border: 1px solid rgba(127,127,127,0.18); }
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
{% endfilter %}
  <h2>{{ t("comments.count", n=comments.count) }}</h2>
  {% for c in comments["items"] %}
  <article class="aegis-comment aegis-depth-{{ c.depth if c.depth < 4 else 4 }}" id="comment-{{ c.id }}">
    <header class="aegis-comment-head">
      {% if c.avatar_url %}<img class="aegis-comment-avatar" src="{{ c.avatar_url }}" alt="" width="28" height="28" loading="lazy" referrerpolicy="no-referrer">{% endif %}
      <strong>{{ c.author_name }}</strong>
      {% if c.staff %}<span class="aegis-comment-badge">{{ t("staff") }}</span>{% endif %}
      <time datetime="{{ c.created_at }}">{{ c.created_at | date_short }}</time>
    </header>
    <div class="aegis-comment-body">{{ c.body_html | safe }}</div>
    {% if comments.open and c.depth < comments.max_depth %}<button type="button" class="aegis-comment-reply" data-reply-to="{{ c.id }}" data-reply-name="{{ c.author_name }}">{{ t("reply") }}</button>{% endif %}
  </article>
  {% else %}
  <p class="aegis-comments-note">{{ t("comments.none") }}</p>
  {% endfor %}
  {% if comments.open %}
  <form class="aegis-comment-form" data-post-url="{{ comments.post_url }}" data-msg-sending="{{ t("comments.sending") }}" data-msg-failed="{{ t("comments.failed") }}" data-msg-pending="{{ t("comments.pending") }}"{% if comments.captcha_html %} data-captcha-field="{{ comments.captcha_field }}"{% endif %}>
    <h3>{{ t("comments.leave") }}</h3>
    <p class="aegis-comments-note" data-replying hidden>{{ t("comments.replying_to") }} <strong data-replying-name></strong> <button type="button" class="aegis-comment-cancel" data-cancel-reply>{{ t("comments.cancel") }}</button></p>
    <input type="hidden" name="parent_id" value="">
    <label>{{ t("comments.name") }} <input name="name" required maxlength="100" autocomplete="name"></label>
    <label>{{ t("comments.email") }} <input type="email" name="email" required maxlength="254" autocomplete="email"></label>
    <label>{{ t("comments.body") }} <textarea name="body" required rows="5" maxlength="{{ comments.max_length }}"></textarea></label>
    <div class="aegis-comment-hp" aria-hidden="true"><label>{{ t("comments.honeypot") }} <input name="website" tabindex="-1" autocomplete="off"></label></div>
    <label class="aegis-comment-check"><input type="checkbox" name="notify_replies"> {{ t("comments.notify") }}</label>
    {% if comments.captcha_html %}<div class="aegis-comment-captcha">{{ comments.captcha_html | safe }}</div>{% endif %}
    <button type="submit">{{ t("comments.submit") }}</button>
    <p class="aegis-comment-status" role="status" aria-live="polite"></p>
  </form>
  {% if comments.script_url %}<script src="{{ comments.script_url | safe }}" defer></script>{% endif %}
  {% else %}
  <p class="aegis-comments-note">{{ t("comments.closed") }}</p>
  {% endif %}
</section>"##;

pub const PRODUCTS_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<section class="aegis-shop">
  {% include "shop_common.html" %}
  <h1 class="aegis-s87">{{ title }}</h1>
  {% if items and items | length > 0 %}
  <div class="aegis-grid">
    {% for item in items %}
    <a class="aegis-card" href="{{ theme_vars.products_url }}/{{ item.slug }}">
      {% if item.fields.image_url %}<img src="{{ item.fields.image_url }}" alt="" loading="lazy">{% else %}<div class="aegis-noimg"></div>{% endif %}
      <div class="aegis-card-body">
        <span class="aegis-card-title">{{ item.title }}</span>
        {% if item.fields.excerpt %}<span class="aegis-muted">{{ item.fields.excerpt | truncate(80) }}</span>{% elif item.excerpt %}<span class="aegis-muted">{{ item.excerpt | truncate(80) }}</span>{% endif %}
        <span class="aegis-s88">
          <span class="aegis-price">{{ item.fields.price | money(theme_vars.currency) }}</span>
          {% if item.fields.compare_at_price %}<span class="aegis-compare">{{ item.fields.compare_at_price | money(theme_vars.currency) }}</span>{% endif %}
        </span>
        {% if item.fields.stock is defined and item.fields.stock is not none and item.fields.stock | int <= 0 %}<span class="aegis-soldout">{{ t("shop.sold_out") }}</span>{% endif %}
      </div>
    </a>
    {% endfor %}
  </div>
  {% else %}
  <div class="aegis-empty"><p>{{ t("shop.no_products") }}</p></div>
  {% endif %}
  {% if pagination and pagination.total_pages > 1 %}
  <nav class="aegis-s89" aria-label="{{ t("pagination.label") }}">
    {% if pagination.prev_url %}<a href="{{ pagination.prev_url }}" class="aegis-button aegis-secondary">&larr; {{ t("pagination.previous") }}</a>{% else %}<span></span>{% endif %}
    <span class="aegis-muted">{{ t("pagination.page", page=pagination.page, total=pagination.total_pages) }}</span>
    {% if pagination.next_url %}<a href="{{ pagination.next_url }}" class="aegis-button aegis-secondary">{{ t("pagination.next") }} &rarr;</a>{% else %}<span></span>{% endif %}
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
        <span class="aegis-price aegis-s90">{{ fields.price | money(theme_vars.currency) }}</span>
        {% if fields.compare_at_price %}<span class="aegis-compare">{{ fields.compare_at_price | money(theme_vars.currency) }}</span>{% endif %}
      </div>
      {% if fields.sku %}<div class="aegis-muted">{{ t("shop.sku", sku=fields.sku) }}</div>{% endif %}
      {% set sold_out = fields.stock is defined and fields.stock is not none and fields.stock | int <= 0 %}
      <div class="aegis-buy">
        {% if sold_out %}
        <span class="aegis-soldout">{{ t("shop.sold_out") }}</span>
        {% elif theme_vars.shop_enabled == "true" %}
        <input id="aegis-qty" class="aegis-qty" type="number" min="1" value="1" aria-label="{{ t("shop.quantity") }}">
        <button type="button" class="aegis-button" data-aegis-add-to-cart data-quantity-input="#aegis-qty"
          data-product-id="{{ fields.id }}" data-product-slug="{{ slug }}" data-product-title="{{ title }}" data-product-price="{{ fields.price }}">{{ t("shop.add_to_cart") }}</button>
        {% elif fields.buy_url %}
        <a class="aegis-button" href="{{ fields.buy_url }}" rel="noopener">{{ t("shop.buy_now") }}</a>
        {% endif %}
        {% if not sold_out and fields.stock is defined and fields.stock is not none and fields.stock | int <= 5 %}<span class="aegis-muted">{{ t("shop.only_left", n=fields.stock) }}</span>{% endif %}
      </div>
      {% if fields.excerpt %}<p class="aegis-s91">{{ fields.excerpt }}</p>{% endif %}
      <div class="prose">{{ content_html | safe }}</div>
    </div>
  </article>
  {% if related_articles and related_articles | length > 0 %}
  <section class="related-articles aegis-s69">
    <h2 class="aegis-s70">
      {{ t("article.related") }}
    </h2>
    <div class="aegis-s71">
      {% for item in related_articles %}
      <a class="aegis-s72" href="{{ base_path }}/articles/{{ item.slug }}">
        <h3 class="aegis-s73">
          {{ item.title }}
        </h3>
        {% if item.excerpt %}
        <p class="aegis-s74">
          {{ item.excerpt }}
        </p>
        {% endif %}
        {% if item.published_at %}
        <time class="aegis-s75" datetime="{{ item.published_at }}">
          {{ item.published_at | date_short }}
        </time>
        {% endif %}
      </a>
      {% endfor %}
    </div>
  </section>
  {% endif %}

  {% if theme_vars.reviews_enabled == "true" %}{% include "reviews.html" %}{% endif %}
  {% if comments %}{% include "comments.html" %}{% endif %}
</section>
{% endblock %}
"##;

/// 商品ページのレビュー欄（`product.html` が `reviews_enabled` のときに読み込む）。中身はスクリプト（`/shop/reviews.js`）が API から作る。
/// 色はテーマの文字色を基準にした半透明で指定する
pub const REVIEWS_TEMPLATE: &str = r##"<section class="aegis-reviews" id="reviews">
{% filter inline_style %}
  .aegis-reviews { margin-top: 3rem; padding-top: 2rem; border-top: 1px solid rgba(127,127,127,0.25); }
  .aegis-reviews h2 { font-size: 1.35rem; margin-bottom: 1rem; }
  .aegis-reviews h3 { font-size: 1.1rem; margin: 1.5rem 0 0.75rem; }
  .aegis-reviews-ul { list-style: none; padding: 0; display: flex; flex-direction: column; gap: 0.9rem; margin: 1rem 0; }
  .aegis-review { padding: 0.9rem 1rem; border-radius: 12px; background: rgba(127,127,127,0.07); border: 1px solid rgba(127,127,127,0.18); }
  .aegis-review-head { display: flex; flex-wrap: wrap; gap: 0.6rem; align-items: baseline; margin-bottom: 0.35rem; }
  .aegis-review-stars { color: #f59e0b; letter-spacing: 0.05em; }
  .aegis-review-body { white-space: pre-line; overflow-wrap: anywhere; margin: 0.25rem 0 0.5rem; }
  .aegis-review-badge { margin-left: 0.5rem; padding: 0.05rem 0.5rem; border-radius: 999px; background: rgba(16,185,129,0.15); color: #10b981; font-size: 0.75rem; font-weight: 600; }
  .aegis-review-reply { margin-top: 0.6rem; padding: 0.6rem 0.8rem; border-left: 3px solid var(--accent, #6366f1); background: rgba(127,127,127,0.06); border-radius: 0 8px 8px 0; }
  .aegis-review-summary { font-size: 1.05rem; }
  .aegis-reviews-form select, .aegis-reviews-form input, .aegis-reviews-form textarea { padding: 0.55rem 0.7rem; border-radius: 10px; border: 1px solid rgba(127,127,127,0.35); background: transparent; color: inherit; font: inherit; }
{% endfilter %}
  <h2>{{ t("reviews.title") }}</h2>
  <div id="aegis-reviews"><p class="aegis-muted">{{ t("reviews.loading") }}</p></div>
  <noscript><p class="aegis-muted">{{ t("reviews.needs_js") }}</p></noscript>
  <script src="{{ theme_vars.reviews_script }}" data-api="{{ theme_vars.reviews_api }}" data-members-api="{{ theme_vars.account_api }}" data-product-id="{{ fields.id }}" data-login-url="{{ theme_vars.account_login_url }}" data-register-url="{{ theme_vars.account_register_url }}" defer></script>
</section>
"##;

pub const CART_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<section class="aegis-shop">
  {% include "shop_common.html" %}
  <h1 class="aegis-s87">{{ title }}</h1>
  <div id="aegis-cart"><p class="aegis-muted">{{ t("shop.cart_loading") }}</p></div>
  <noscript><p class="aegis-error">{{ t("shop.cart_needs_js") }}</p></noscript>
</section>
{% endblock %}
"#;

/// 会員アカウントのページ（`/account`・`/account/login`・`/account/register`）。`slug` が表示（`home` / `login` / `register`）。
/// 中身はスクリプト（`/shop/account.js`）が API から作る。ログインは HttpOnly の Cookie に持つので、スクリプトはトークンに触れない
pub const ACCOUNT_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<section class="aegis-shop aegis-s92">
  {% include "shop_common.html" %}
  <h1 class="aegis-s87">{{ t("account.title") }}</h1>
  <div id="aegis-account" data-view="{{ slug }}"><p class="aegis-muted">{{ t("account.loading") }}</p></div>
  <noscript><p class="aegis-muted">{{ t("account.needs_js") }}</p></noscript>
  <script src="{{ theme_vars.account_script }}" data-api="{{ theme_vars.account_api }}" data-account-url="{{ theme_vars.account_url }}" data-login-url="{{ theme_vars.account_login_url }}" data-register-url="{{ theme_vars.account_register_url }}" data-forgot-url="{{ theme_vars.account_forgot_url }}" data-min-password="{{ theme_vars.account_min_password }}" defer></script>
</section>
{% endblock %}
"#;

pub const ORDER_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<section class="aegis-shop aegis-s92">
  {% include "shop_common.html" %}
  <h1 class="aegis-s87">{{ t("shop.order_thanks") }}</h1>
  <div id="aegis-order" data-order-id="{{ slug }}"><p class="aegis-muted">{{ t("shop.order_loading") }}</p></div>
</section>
{% endblock %}
"#;

/// フォーラム共通スタイル・ナビゲーション
pub const FORUM_COMMON_TEMPLATE: &str = r##"{% filter inline_style %}
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
{% endfilter %}
"##;

/// フォーラムトップ画面（カテゴリ一覧 & 最近のスレッド）
pub const FORUM_INDEX_TEMPLATE: &str = r##"{% extends "base.html" %}

{% block content %}
{% include "forum_common.html" %}
<section class="aegis-forum">
  <nav class="aegis-forum-nav" aria-label="{{ t("forum.label") }}">
    <div class="aegis-forum-breadcrumbs">
      <strong>{{ t("forum.title") }}</strong>
    </div>
  </nav>

  <h1 class="aegis-s93">{{ t("forum.title") }}</h1>
  <p class="aegis-s94">{{ t("forum.tagline") }}</p>

  <h2 class="aegis-s95">{{ t("forum.categories") }}</h2>
  <div class="aegis-forum-grid">
    {% for cat in fields.categories %}
    <a href="{{ theme_vars.forum_url }}/{{ cat.slug }}" class="aegis-forum-card">
      <div class="aegis-forum-cat-title">
        <span>{{ cat.name }}</span>
        <span class="aegis-s96">&rarr;</span>
      </div>
      <p class="aegis-forum-cat-desc">{{ cat.description }}</p>
      <div class="aegis-forum-cat-stats">
        <span>{{ t("forum.threads_count", n=cat.thread_count) }}</span>
        <span>{{ t("forum.posts_count", n=cat.post_count) }}</span>
      </div>
    </a>
    {% else %}
    <p class="aegis-forum-empty">{{ t("forum.no_categories") }}</p>
    {% endfor %}
  </div>

  {% if fields.recent_threads | length > 0 %}
  <h2 class="aegis-s97">{{ t("forum.recent") }}</h2>
  <div class="aegis-forum-threads">
    {% for thread in fields.recent_threads %}
    <a href="{{ theme_vars.forum_url }}/{{ thread.category_slug }}/{{ thread.id }}" class="aegis-forum-thread-item">
      <div class="aegis-forum-thread-main">
        <div class="aegis-forum-thread-title">
          {% if thread.is_pinned %}<span class="aegis-forum-badge aegis-forum-badge-pinned">{{ t("forum.pin") }}</span>{% endif %}
          {% if thread.is_locked %}<span class="aegis-forum-badge aegis-forum-badge-locked">{{ t("forum.locked") }}</span>{% endif %}
          <span>{{ thread.title }}</span>
        </div>
        <div class="aegis-forum-thread-meta">
          <span>{{ t("forum.in_category", name=thread.category_name) }}</span>
          <span>{{ t("forum.by", name=thread.author_name) }}</span>
          <span>{{ t("forum.last_active") }} <time datetime="{{ thread.last_post_at }}">{{ thread.last_post_at | datetime_short }}</time></span>
        </div>
      </div>
      <div class="aegis-forum-badge-replies" title="{{ t("forum.total_replies") }}">
        {{ thread.post_count }}
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
  <nav class="aegis-forum-nav" aria-label="{{ t("forum.label") }}">
    <div class="aegis-forum-breadcrumbs">
      <a href="{{ theme_vars.forum_url }}">{{ t("forum.name") }}</a>
      <span>&gt;</span>
      <strong>{{ fields.category.name }}</strong>
    </div>
    <a href="#new-thread" class="aegis-forum-btn">{{ t("forum.new_thread") }}</a>
  </nav>

  <h1 class="aegis-s98">{{ fields.category.name }}</h1>
  {% if fields.category.description %}
  <p class="aegis-s99">{{ fields.category.description }}</p>
  {% endif %}
  {% if fields.pending_notice %}
  <div class="aegis-forum-banner-notice" role="status">{{ t("forum.thread_pending") }}</div>
  {% endif %}

  <div class="aegis-forum-threads">
    {% for thread in fields.threads %}
    <a href="{{ theme_vars.forum_url }}/{{ fields.category.slug }}/{{ thread.id }}" class="aegis-forum-thread-item">
      <div class="aegis-forum-thread-main">
        <div class="aegis-forum-thread-title">
          {% if thread.is_pinned %}<span class="aegis-forum-badge aegis-forum-badge-pinned">{{ t("forum.pin") }}</span>{% endif %}
          {% if thread.is_locked %}<span class="aegis-forum-badge aegis-forum-badge-locked">{{ t("forum.locked") }}</span>{% endif %}
          <span>{{ thread.title }}</span>
        </div>
        <div class="aegis-forum-thread-meta">
          <span>{{ t("forum.started_by", name=thread.author_name) }}</span>
          <span>{{ t("forum.last_post") }} <time datetime="{{ thread.last_post_at }}">{{ thread.last_post_at | datetime_short }}</time></span>
        </div>
      </div>
      <div class="aegis-forum-badge-replies" title="{{ t("forum.total_replies") }}">
        {{ thread.post_count }}
      </div>
    </a>
    {% else %}
    <div class="aegis-forum-card aegis-forum-empty">
      {{ t("forum.no_threads") }}
    </div>
    {% endfor %}
  </div>

  {% if pagination %}
  <div class="aegis-s100">
    {% if pagination.prev_url %}
    <a href="{{ pagination.prev_url }}" class="aegis-forum-btn aegis-forum-btn-secondary">&larr; {{ t("pagination.previous") }}</a>
    {% endif %}
    {% if pagination.next_url %}
    <a href="{{ pagination.next_url }}" class="aegis-forum-btn aegis-forum-btn-secondary">{{ t("pagination.next") }} &rarr;</a>
    {% endif %}
  </div>
  {% endif %}

  <section class="aegis-forum-form" id="new-thread">
    <h3>{{ t("forum.start_thread") }}</h3>
    <form method="POST" action="{{ theme_vars.forum_url }}/{{ fields.category.slug }}/threads">
      <div class="aegis-forum-form-group">
        <label for="thread-title">{{ t("forum.thread_title") }}</label>
        <input type="text" id="thread-title" name="title" required minlength="2" maxlength="200" placeholder="{{ t("forum.thread_title_hint") }}">
      </div>
      {% if theme_vars.forum_members == "true" %}
      <p class="aegis-forum-note">{% if theme_vars.forum_members_only == "true" %}{{ t("forum.members_only") }}{% else %}{{ t("forum.member_hint") }}{% endif %} <a href="{{ theme_vars.forum_login_url }}">{{ t("forum.login") }}</a></p>
      {% endif %}
      <div class="aegis-forum-form-group">
        <label for="thread-author">{{ t("forum.your_name") }}</label>
        <input type="text" id="thread-author" name="author_name" placeholder="{{ t("forum.anonymous") }}" maxlength="50">
      </div>
      <div class="aegis-forum-form-group">
        <label for="thread-body">{{ t("forum.message_body") }}</label>
        <textarea id="thread-body" name="body" rows="6" required minlength="1" placeholder="{{ t("forum.message_hint") }}"></textarea>
      </div>
      {% if fields.captcha_html %}<div class="aegis-forum-form-group">{{ fields.captcha_html | safe }}</div>{% endif %}
      <button type="submit" class="aegis-forum-btn">{{ t("forum.create_thread") }}</button>
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
  <nav class="aegis-forum-nav" aria-label="{{ t("forum.label") }}">
    <div class="aegis-forum-breadcrumbs">
      <a href="{{ theme_vars.forum_url }}">{{ t("forum.name") }}</a>
      <span>&gt;</span>
      <a href="{{ theme_vars.forum_url }}/{{ fields.category.slug }}">{{ fields.category.name }}</a>
      <span>&gt;</span>
      <strong class="aegis-s101">{{ fields.thread.title }}</strong>
    </div>
    {% if not fields.thread.is_locked %}
    <a href="#reply-form" class="aegis-forum-btn">{{ t("reply") }}</a>
    {% endif %}
  </nav>

  <header class="aegis-s102">
    <div class="aegis-s103">
      {% if fields.thread.is_pinned %}<span class="aegis-forum-badge aegis-forum-badge-pinned">{{ t("forum.pinned") }}</span>{% endif %}
      {% if fields.thread.is_locked %}<span class="aegis-forum-badge aegis-forum-badge-locked">{{ t("forum.locked") }}</span>{% endif %}
      <h1 class="aegis-s104">{{ fields.thread.title }}</h1>
    </div>
    <div class="aegis-s105">
      <span>{{ t("forum.started_by", name=fields.thread.author_name) }}</span>
      <span>{{ t("forum.posts", n=fields.thread.post_count) }}</span>
      <span>{{ t("forum.created") }} <time datetime="{{ fields.thread.created_at }}">{{ fields.thread.created_at | date_short }}</time></span>
    </div>
  </header>

  {% if fields.pending_notice %}
  <div class="aegis-forum-banner-notice" role="status">{{ t("forum.reply_pending") }}</div>
  {% endif %}
  {% if fields.reported_notice %}
  <div class="aegis-forum-banner-notice" role="status">{{ t("forum.reported") }}</div>
  {% endif %}

  <div class="aegis-forum-posts">
    {% for p in fields.posts %}
    <article class="aegis-forum-post" id="post-{{ p.post_number }}">
      <div class="aegis-forum-post-head">
        <div class="aegis-forum-post-author">
          {% if p.avatar_url %}<img class="aegis-forum-avatar aegis-s106" src="{{ p.avatar_url }}" alt="" width="28" height="28">{% else %}<div class="aegis-forum-avatar">{{ p.author_initial }}</div>{% endif %}
          <span>{{ p.author_name }}</span>
          {% if p.is_author_staff %}
          <span class="aegis-forum-badge aegis-s107">{{ t("staff") }}</span>
          {% endif %}
        </div>
        <div class="aegis-forum-post-meta">
          <time datetime="{{ p.created_at }}">{{ p.created_at | datetime_short }}</time>
          <a class="aegis-s108" href="#post-{{ p.post_number }}">#{{ p.post_number }}</a>
        </div>
      </div>
      {% if p.reply_to_number %}
      <p class="aegis-forum-reply-to"><a href="#post-{{ p.reply_to_number }}">&gt;&gt;{{ p.reply_to_number }}</a></p>
      {% endif %}
      <div class="aegis-forum-post-body">
        {{ p.body_html | safe }}
      </div>
      <details class="aegis-forum-report">
        <summary>{{ t("forum.report") }}</summary>
        <form class="aegis-forum-report-form" method="POST" action="{{ theme_vars.forum_url }}/{{ fields.category.slug }}/{{ fields.thread.id }}/report">
          <input type="hidden" name="target_type" value="post">
          <input type="hidden" name="target_id" value="{{ p.id }}">
          <input type="text" name="reason" required maxlength="500" placeholder="{{ t("forum.report_reason") }}">
          <button type="submit" class="aegis-forum-btn aegis-forum-btn-secondary aegis-s109">{{ t("forum.report") }}</button>
        </form>
      </details>
    </article>
    {% endfor %}
  </div>

  {% if pagination %}
  <div class="aegis-s100">
    {% if pagination.prev_url %}
    <a href="{{ pagination.prev_url }}" class="aegis-forum-btn aegis-forum-btn-secondary">&larr; {{ t("pagination.previous") }}</a>
    {% endif %}
    {% if pagination.next_url %}
    <a href="{{ pagination.next_url }}" class="aegis-forum-btn aegis-forum-btn-secondary">{{ t("pagination.next") }} &rarr;</a>
    {% endif %}
  </div>
  {% endif %}

  {% if fields.thread.is_locked %}
  <div class="aegis-forum-banner-locked">
    {{ t("forum.locked_banner") }}
  </div>
  {% else %}
  <section class="aegis-forum-form" id="reply-form">
    <h3>{{ t("forum.post_reply") }}</h3>
    <form method="POST" action="{{ theme_vars.forum_url }}/{{ fields.category.slug }}/{{ fields.thread.id }}/posts">
      {% if theme_vars.forum_members == "true" %}
      <p class="aegis-forum-note">{% if theme_vars.forum_members_only == "true" %}{{ t("forum.members_only") }}{% else %}{{ t("forum.member_hint") }}{% endif %} <a href="{{ theme_vars.forum_login_url }}">{{ t("forum.login") }}</a></p>
      {% endif %}
      <div class="aegis-forum-form-group">
        <label for="post-author">{{ t("forum.your_name") }}</label>
        <input type="text" id="post-author" name="author_name" placeholder="{{ t("forum.anonymous") }}" maxlength="50">
      </div>
      <div class="aegis-forum-form-group">
        <label for="post-body">{{ t("forum.reply_content") }}</label>
        <textarea id="post-body" name="body" rows="5" required minlength="1" placeholder="{{ t("forum.reply_hint") }}"></textarea>
      </div>
      {% if fields.captcha_html %}<div class="aegis-forum-form-group">{{ fields.captcha_html | safe }}</div>{% endif %}
      <button type="submit" class="aegis-forum-btn">{{ t("forum.submit_reply") }}</button>
    </form>
  </section>
  {% endif %}
</section>
{% endblock %}
"##;

/// 組み込みのテンプレートが `style=""` の代わりに使うクラス（Strict CSP では `style` 属性を使わない。docs/csp.md 6.2）。
/// 元の `style` 属性と同じく、ほかのスタイルより優先する（`!important`）。`render.rs` が `extra_head` に入れる
pub const UTILITY_STYLES: &str = "<style id=\"aegis-utility-styles\">.aegis-s1{margin-bottom: 3.5rem !important;text-align: center !important;padding: 2.5rem 1rem !important}\n.aegis-s2{display: inline-block !important;background: rgba(99, 102, 241, 0.1) !important;border: 1px solid rgba(99, 102, 241, 0.25) !important;color: #a5b4fc !important;padding: 0.35rem 0.85rem !important;border-radius: 9999px !important;font-size: 0.8rem !important;font-weight: 600 !important;margin-bottom: 1.25rem !important}\n.aegis-s3{font-size: 2.85rem !important;font-weight: 800 !important;letter-spacing: -0.03em !important;margin-bottom: 1rem !important;background: linear-gradient(135deg, #ffffff 30%, #9ca3af 100%) !important;-webkit-background-clip: text !important;-webkit-text-fill-color: transparent !important}\n.aegis-s4{font-size: 1.15rem !important;color: var(--text-muted) !important;max-width: 620px !important;margin: 0 auto !important;line-height: 1.6 !important}\n.aegis-s5{display: flex !important;align-items: center !important;justify-content: space-between !important;margin-bottom: 1.75rem !important;border-bottom: 1px solid var(--border-color) !important;padding-bottom: 0.75rem !important}\n.aegis-s6{font-size: 1.35rem !important;font-weight: 700 !important;color: #ffffff !important}\n.aegis-s7{font-size: 0.85rem !important;color: var(--text-dim) !important}\n.aegis-s8{display: grid !important;grid-template-columns: repeat(auto-fill, minmax(min(320px, 100%), 1fr)) !important;gap: 1.75rem !important}\n.aegis-s9{background: var(--bg-card) !important;border: 1px solid var(--border-color) !important;border-radius: var(--radius-md) !important;padding: 1.5rem !important;display: flex !important;flex-direction: column !important;justify-content: space-between !important;transition: all 0.25s ease !important;position: relative !important;overflow: hidden !important;backdrop-filter: blur(8px) !important}\n.aegis-s10{display: flex !important;align-items: center !important;gap: 0.6rem !important;margin-bottom: 0.85rem !important}\n.aegis-s11{background: rgba(99, 102, 241, 0.15) !important;color: #818cf8 !important;font-size: 0.75rem !important;font-weight: 600 !important;padding: 0.2rem 0.55rem !important;border-radius: var(--radius-sm) !important;text-transform: uppercase !important}\n.aegis-s12{font-size: 0.8rem !important;color: var(--text-dim) !important}\n.aegis-s13{font-size: 1.25rem !important;font-weight: 700 !important;margin-bottom: 0.65rem !important;line-height: 1.4 !important}\n.aegis-s14{color: #ffffff !important;text-decoration: none !important;transition: color 0.15s ease !important}\n.aegis-s15{font-size: 0.92rem !important;color: var(--text-muted) !important;line-height: 1.6 !important;margin-bottom: 1.25rem !important}\n.aegis-s16{padding-top: 1rem !important;border-top: 1px solid var(--border-color) !important;display: flex !important;align-items: center !important;justify-content: space-between !important}\n.aegis-s17{font-size: 0.85rem !important;font-weight: 600 !important;color: #818cf8 !important;text-decoration: none !important;display: flex !important;align-items: center !important;gap: 0.25rem !important}\n.aegis-s18{text-align: center !important;padding: 4rem 1.5rem !important;background: var(--bg-card) !important;border: 1px dashed var(--border-color) !important;border-radius: var(--radius-lg) !important}\n.aegis-s19{width: 48px !important;height: 48px !important;background: rgba(255, 255, 255, 0.05) !important;border-radius: 50% !important;display: flex !important;align-items: center !important;justify-content: center !important;margin: 0 auto 1rem auto !important;color: var(--text-dim) !important}\n.aegis-s20{font-size: 1.2rem !important;font-weight: 600 !important;margin-bottom: 0.5rem !important;color: #ffffff !important}\n.aegis-s21{font-size: 0.9rem !important;color: var(--text-muted) !important;margin-bottom: 1.5rem !important}\n.aegis-s22{display: inline-block !important;padding: 0.5rem 1.25rem !important}\n.aegis-s23{display: flex !important;align-items: center !important;justify-content: space-between !important;gap: 1rem !important;margin-top: 2.5rem !important;color: var(--text-dim) !important;font-size: 0.85rem !important}\n.aegis-s24{max-width: 820px !important;margin: 0 auto !important;padding: 1rem 0 3rem !important}\n.aegis-s25{font-size: 0.8rem !important;letter-spacing: 0.08em !important;text-transform: uppercase !important;opacity: 0.65 !important;margin: 0 0 0.4rem !important}\n.aegis-s26{margin: 0 0 0.75rem !important}\n.aegis-s27{opacity: 0.75 !important;margin: 0 0 2rem !important}\n.aegis-s28{list-style: none !important;padding: 0 !important;margin: 0 !important}\n.aegis-s29{padding: 1rem 0 !important;border-top: 1px solid rgba(127, 127, 127, 0.25) !important}\n.aegis-s30{font-size: 0.85rem !important;opacity: 0.65 !important}\n.aegis-s31{font-size: 1.15rem !important;margin: 0.2rem 0 0.3rem !important}\n.aegis-s32{color: inherit !important}\n.aegis-s33{opacity: 0.75 !important;margin: 0 !important}\n.aegis-s34{padding: 1rem 0 !important;border-top: 1px solid rgba(127, 127, 127, 0.25) !important;opacity: 0.7 !important}\n.aegis-s35{display: flex !important;justify-content: space-between !important;gap: 1rem !important;margin-top: 2rem !important;font-size: 0.9rem !important}\n.aegis-s36{opacity: 0.65 !important}\n.aegis-s37{margin-top: 2rem !important;font-size: 0.85rem !important}\n.aegis-s38{margin: 0 0 1.5rem !important}\n.aegis-s39{padding: 0.75rem 0 !important;border-top: 1px solid rgba(127, 127, 127, 0.25) !important}\n.aegis-s40{color: inherit !important;font-weight: 600 !important}\n.aegis-s41{opacity: 0.65 !important;font-size: 0.85rem !important}\n.aegis-s42{opacity: 0.75 !important;margin: 0.25rem 0 0 !important}\n.aegis-s43{max-width: 720px !important;margin: 0 auto !important;padding: 1rem 0 3rem !important}\n.aegis-s44{margin: 0 0 1rem !important}\n.aegis-s45{margin: 0 !important}\n.aegis-s46{display: flex !important;gap: 0.5rem !important;margin: 0 0 1.5rem !important}\n.aegis-s47{flex: 1 !important;min-width: 0 !important;padding: 0.55rem 0.75rem !important;font: inherit !important;border: 1px solid rgba(127, 127, 127, 0.45) !important;border-radius: 6px !important;background: transparent !important;color: inherit !important}\n.aegis-s48{padding: 0.55rem 0.5rem !important;font: inherit !important;border: 1px solid rgba(127, 127, 127, 0.45) !important;border-radius: 6px !important;background: transparent !important;color: inherit !important}\n.aegis-s49{padding: 0.55rem 1rem !important;font: inherit !important;border: 1px solid rgba(127, 127, 127, 0.45) !important;border-radius: 6px !important;background: transparent !important;color: inherit !important;cursor: pointer !important}\n.aegis-s50{opacity: 0.7 !important;margin: 0 0 0.5rem !important;font-size: 0.9rem !important}\n.aegis-s51{font-size: 1.15rem !important;margin: 0 0 0.3rem !important}\n.aegis-s52{opacity: 0.8 !important;margin: 0 !important}\n.aegis-s53{font-size: 0.8rem !important;opacity: 0.6 !important}\n.aegis-s54{max-width: 820px !important;margin: 0 auto !important}\n.aegis-s55{display: flex !important;align-items: center !important;gap: 0.5rem !important;font-size: 0.85rem !important;color: var(--text-dim) !important;margin-bottom: 1.75rem !important}\n.aegis-s56{color: var(--text-dim) !important;text-decoration: none !important}\n.aegis-s57{text-transform: capitalize !important}\n.aegis-s58{color: var(--text-muted) !important;overflow: hidden !important;text-overflow: ellipsis !important;white-space: nowrap !important}\n.aegis-s59{margin-bottom: 2.5rem !important;border-bottom: 1px solid var(--border-color) !important;padding-bottom: 1.75rem !important}\n.aegis-s60{display: flex !important;align-items: center !important;gap: 0.75rem !important;margin-bottom: 1rem !important}\n.aegis-s61{background: rgba(99, 102, 241, 0.15) !important;color: #818cf8 !important;font-size: 0.78rem !important;font-weight: 600 !important;padding: 0.25rem 0.65rem !important;border-radius: var(--radius-sm) !important;text-transform: uppercase !important}\n.aegis-s62{font-size: 2.65rem !important;font-weight: 800 !important;letter-spacing: -0.03em !important;line-height: 1.25 !important;color: #ffffff !important;margin-bottom: 1.25rem !important}\n.aegis-s63{display: flex !important;align-items: center !important;gap: 1rem !important}\n.aegis-s64{width: 36px !important;height: 36px !important;border-radius: 50% !important;background: var(--accent-gradient) !important;display: flex !important;align-items: center !important;justify-content: center !important;font-weight: 700 !important;font-size: 0.85rem !important;color: #ffffff !important}\n.aegis-s65{font-size: 0.9rem !important;font-weight: 600 !important;color: #ffffff !important}\n.aegis-s66{font-size: 0.78rem !important;color: var(--text-dim) !important}\n.aegis-s67{list-style: none !important;display: flex !important;flex-wrap: wrap !important;gap: 0.4rem !important;padding: 0 !important;margin: 1rem 0 0 !important}\n.aegis-s68{display: inline-block !important;padding: 0.15rem 0.6rem !important;border: 1px solid rgba(127, 127, 127, 0.4) !important;border-radius: 999px !important;font-size: 0.8rem !important;color: inherit !important;text-decoration: none !important}\n.aegis-s69{margin-top: 3.5rem !important;padding-top: 2.5rem !important;border-top: 1px solid var(--border-color) !important}\n.aegis-s70{font-size: 1.4rem !important;font-weight: 700 !important;color: #ffffff !important;margin-bottom: 1.25rem !important}\n.aegis-s71{display: grid !important;grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)) !important;gap: 1rem !important}\n.aegis-s72{display: flex !important;flex-direction: column !important;padding: 1.25rem !important;background: var(--bg-card) !important;border: 1px solid var(--border-color) !important;border-radius: var(--radius-md) !important;text-decoration: none !important;color: inherit !important;transition: border-color 0.2s, transform 0.2s !important}\n.aegis-s73{font-size: 1.05rem !important;font-weight: 600 !important;color: #ffffff !important;margin-bottom: 0.5rem !important;line-height: 1.4 !important}\n.aegis-s74{font-size: 0.85rem !important;color: var(--text-muted) !important;line-height: 1.5 !important;margin-bottom: 0.75rem !important;display: -webkit-box !important;-webkit-line-clamp: 2 !important;-webkit-box-orient: vertical !important;overflow: hidden !important}\n.aegis-s75{font-size: 0.75rem !important;color: var(--text-dim) !important;margin-top: auto !important}\n.aegis-s76{margin-top: 3.5rem !important;padding-top: 2rem !important;border-top: 1px solid var(--border-color) !important;display: flex !important;align-items: center !important;justify-content: space-between !important}\n.aegis-s77{color: #818cf8 !important;font-size: 0.9rem !important;text-decoration: none !important;font-weight: 600 !important}\n.aegis-s78{background: transparent !important;border: 1px solid var(--border-color) !important;color: var(--text-muted) !important;padding: 0.4rem 0.85rem !important;border-radius: var(--radius-sm) !important;font-size: 0.8rem !important;cursor: pointer !important;text-decoration: none !important;transition: all 0.2s ease !important}\n.aegis-s79{margin-bottom: 2rem !important;border-bottom: 1px solid var(--border-color) !important;padding-bottom: 1.25rem !important}\n.aegis-s80{font-size: 2.5rem !important;font-weight: 800 !important;letter-spacing: -0.02em !important;color: #ffffff !important}\n.aegis-s81{list-style: none !important;display: flex !important;flex-wrap: wrap !important;gap: 0.4rem !important;padding: 0 !important;margin: 0 0 1.5rem !important}\n.aegis-s82{text-align: center !important;padding: 5rem 1rem !important}\n.aegis-s83{font-size: 5rem !important;font-weight: 900 !important;background: var(--accent-gradient) !important;-webkit-background-clip: text !important;-webkit-text-fill-color: transparent !important;margin-bottom: 1rem !important;line-height: 1 !important}\n.aegis-s84{font-size: 1.75rem !important;font-weight: 700 !important;color: #ffffff !important;margin-bottom: 0.75rem !important}\n.aegis-s85{font-size: 1rem !important;color: var(--text-muted) !important;max-width: 480px !important;margin: 0 auto 2rem auto !important}\n.aegis-s86{background: var(--accent-gradient) !important;color: #ffffff !important;text-decoration: none !important;padding: 0.65rem 1.5rem !important;border-radius: var(--radius-sm) !important;font-weight: 600 !important;font-size: 0.95rem !important;display: inline-block !important}\n.aegis-s87{font-size: 2rem !important;margin-bottom: 1.5rem !important}\n.aegis-s88{margin-top: auto !important}\n.aegis-s89{display: flex !important;justify-content: space-between !important;margin-top: 2rem !important}\n.aegis-s90{font-size: 1.5rem !important}\n.aegis-s91{margin-bottom: 1rem !important}\n.aegis-s92{max-width: 720px !important}\n.aegis-s93{font-size: 2.2rem !important;margin: 0 0 0.5rem !important;font-weight: 800 !important;letter-spacing: -0.02em !important}\n.aegis-s94{opacity: 0.7 !important;margin: 0 0 2.5rem !important;font-size: 1.05rem !important}\n.aegis-s95{font-size: 1.35rem !important;font-weight: 700 !important;margin: 0 0 1.25rem !important}\n.aegis-s96{opacity: 0.5 !important;font-size: 0.9rem !important}\n.aegis-s97{font-size: 1.35rem !important;font-weight: 700 !important;margin: 2rem 0 1.25rem !important}\n.aegis-s98{font-size: 2rem !important;margin: 0 0 0.5rem !important;font-weight: 800 !important}\n.aegis-s99{opacity: 0.7 !important;margin: 0 0 2rem !important;font-size: 1rem !important}\n.aegis-s100{display: flex !important;justify-content: center !important;gap: 0.5rem !important;margin: 2rem 0 !important}\n.aegis-s101{max-width: 300px !important;white-space: nowrap !important;overflow: hidden !important;text-overflow: ellipsis !important}\n.aegis-s102{margin-bottom: 2rem !important}\n.aegis-s103{display: flex !important;align-items: center !important;gap: 0.6rem !important;margin-bottom: 0.5rem !important;flex-wrap: wrap !important}\n.aegis-s104{font-size: 1.85rem !important;margin: 0 !important;font-weight: 800 !important;line-height: 1.3 !important}\n.aegis-s105{font-size: 0.85rem !important;opacity: 0.65 !important;display: flex !important;gap: 1rem !important;flex-wrap: wrap !important}\n.aegis-s106{object-fit: cover !important}\n.aegis-s107{background: var(--accent, #6366f1) !important;color: #fff !important}\n.aegis-s108{font-weight: 700 !important;opacity: 0.8 !important}\n.aegis-s109{padding: 0.4rem 0.8rem !important;font-size: 0.8rem !important}\n.aegis-depth-1{margin-left:1.5rem !important}\n.aegis-depth-2{margin-left:3rem !important}\n.aegis-depth-3{margin-left:4.5rem !important}\n.aegis-depth-4{margin-left:6rem !important}</style>";
