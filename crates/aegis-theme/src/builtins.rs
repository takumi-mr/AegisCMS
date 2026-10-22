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
      <a href="/" class="brand-logo">
        <div class="logo-icon">
          <svg viewBox="0 0 24 24"><path d="M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm0 2.18l7 3.12v4.7c0 4.67-3.13 9.03-7 10.18-3.87-1.15-7-5.51-7-10.18V6.3l7-3.12z"/></svg>
        </div>
        <span>{{ site_title }}</span>
      </a>
      <nav class="nav-links">
        <a href="/" class="nav-link">Home</a>
        <a href="/admin" class="admin-pill">Admin Studio</a>
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
    <span style="font-size: 0.85rem; color: var(--text-dim);">{{ items | length }} posts</span>
  </div>

  {% if items and items | length > 0 %}
  <div class="cards-grid" style="display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 1.75rem;">
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
          <a href="/articles/{{ item.slug }}" style="color: #ffffff; text-decoration: none; transition: color 0.15s ease;">
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
        <a href="/articles/{{ item.slug }}" style="font-size: 0.85rem; font-weight: 600; color: #818cf8; text-decoration: none; display: flex; align-items: center; gap: 0.25rem;">
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
    <a href="/admin" class="admin-pill" style="display: inline-block; padding: 0.5rem 1.25rem;">Launch Admin Studio</a>
  </div>
  {% endif %}
</section>
{% endblock %}
"#;

pub const ARTICLE_TEMPLATE: &str = r#"{% extends "base.html" %}

{% block content %}
<article class="article-detail" style="max-width: 820px; margin: 0 auto;">
  <nav class="breadcrumb" style="display: flex; align-items: center; gap: 0.5rem; font-size: 0.85rem; color: var(--text-dim); margin-bottom: 1.75rem;">
    <a href="/" style="color: var(--text-dim); text-decoration: none;">Home</a>
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

  <footer class="article-footer" style="margin-top: 3.5rem; padding-top: 2rem; border-top: 1px solid var(--border-color); display: flex; align-items: center; justify-content: space-between;">
    <a href="/" style="color: #818cf8; font-size: 0.9rem; text-decoration: none; font-weight: 600;">
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
</article>
{% endblock %}
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
  <a href="/" style="background: var(--accent-gradient); color: #ffffff; text-decoration: none; padding: 0.65rem 1.5rem; border-radius: var(--radius-sm); font-weight: 600; font-size: 0.95rem; display: inline-block;">
    Return to Homepage
  </a>
</div>
{% endblock %}
"#;
