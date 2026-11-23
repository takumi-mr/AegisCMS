[← Back to the documentation index](README.md)

**English** | [日本語](wordpress-migration.ja.md)

# Moving from WordPress

`aegis-server import wordpress` reads a WordPress export file and creates the content in a running AegisCMS. It does not aim to be a one-button migration: it moves what can be moved and writes a report of what you still have to fix by hand.

## What You Need

1. In WordPress, open **Tools → Export**, choose **All content**, and download the XML file (WXR).
2. A running AegisCMS server and the login of an **admin** user. An API token is not enough: tokens stop at `editor`, and the import creates users and keeps the original publication dates, which only admins can do. A user with two-factor authentication cannot be used; use another admin for the import.
3. The old site must still be online while you import, because the images are downloaded from it.

## Try It First

```bash
aegis-server import wordpress export.xml --dry-run
```

`--dry-run` touches no server. It writes `wordpress-import-report.md` with how many items would be created, which posts contain things that cannot be converted (shortcodes, HTML that was kept as it is), and what would be skipped.

## Import

```bash
export AEGIS_PASSWORD='...'
aegis-server import wordpress export.xml --url http://127.0.0.1:8080 --username admin
```

The same file can be imported again: items that already exist (same slug) are skipped, so you can fix a problem and run the command once more. Items that failed are listed in the report and the command exits with an error.

| Option            | What it does                                                                                |
| ----------------- | ------------------------------------------------------------------------------------------- |
| `--url`           | The server to import into (`AEGIS_URL`, default `http://127.0.0.1:8080`)                    |
| `--username`      | The admin to log in as (`AEGIS_USERNAME`). The password is `--password` or `AEGIS_PASSWORD` |
| `--dry-run`       | Report only                                                                                 |
| `--report <file>` | Where to write the report (default `wordpress-import-report.md`)                            |
| `--skip-media`    | Do not download the images; the content keeps pointing at the old site                      |
| `--skip-authors`  | Do not create users; every item is authored by the admin who runs the import                |

## What Is Moved

| WordPress                  | AegisCMS                                                                                                                                                                          |
| -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Posts                      | `article`, with the original publication date, excerpt, first category and tags                                                                                                   |
| Pages                      | `page`                                                                                                                                                                            |
| Categories                 | `category` content                                                                                                                                                                |
| Tags                       | Tags. A tag whose slug is not ASCII (for example Japanese) gets the slug `wp-tag-<ID>`; the name is kept                                                                          |
| Media                      | Uploaded under `wordpress/` keeping the year and month folders; links in the content (including resized copies such as `-300x200`) are rewritten                                  |
| Authors                    | Users with the role `contributor`, created **disabled** with a random password. Enable them and set a password to let them log in                                                 |
| Drafts, pending, scheduled | The same status                                                                                                                                                                   |
| Password-protected posts   | Imported with their original status and `access = "password"` (password protection)                                                                                               |
| Private posts              | Imported as **drafts**, so nothing private becomes public. They are listed in the report                                                                                          |
| Trash                      | Not imported                                                                                                                                                                      |
| Comments                   | Imported with their status (approved, pending, spam, trash), date and replies, only on published posts. No email is sent. Needs `[comments]` enabled for the content type         |
| Old URLs                   | 301 redirects from the old permalink to the new URL. A big site gets one pattern rule for the permalink structure (for example `/{p1}/{p2}/{slug}`) instead of thousands of rules |

Posts are converted to blocks: paragraphs, headings, lists, quotes, code, tables, images, separators and details. Things that cannot be converted exactly are kept as an HTML block and listed in the report. An image caption becomes an italic paragraph under the image, because the image block cannot hold one. Line breaks inside a paragraph, `<sup>`, and nested lists are kept as a `core/html` block.

## What Is Not Moved

- PHP themes and plugins. The look of a theme can be carried over: see [Moving the Theme](#moving-the-theme). The report says what the content used.
- Shortcodes other than `[caption]`, `[embed]` and `[gallery]`, page-builder content, WooCommerce products, form plugins, and custom post types.
- Featured images, parent categories and parent pages (the structure is flat), and SEO plugin fields.
- Pingbacks and trackbacks, comment authors that were logged-in users (they become plain names), and old URLs of the form `/?p=123` (a query on the home page cannot be redirected).

## Moving the Theme

A WordPress theme is PHP, and PHP cannot run in an AegisCMS theme. What can be carried over is the **look**: the CSS, the images and the fonts. The `wordpress-classic` starter theme outputs the same class names as classic WordPress themes (`site-header`, `entry-title`, `entry-content`, `hentry`, `widget`, `nav-links`, and so on), so the old `style.css` applies to it with little or no change.

```bash
aegis-server theme import-wordpress path/to/wp-content/themes/my-old-theme
aegis-server theme dev themes/my-old-theme
```

`theme import-wordpress` makes `themes/<id>/` (the ID comes from the theme's Text Domain, or use `--id`):

- the starter's templates and `style.css`
- the old theme's files (`style.css`, images, fonts, other CSS and JavaScript) copied to `assets/wp/` with the same folder structure, so `url(images/bg.png)` keeps working. PHP files, translations, `node_modules` and hidden files are not copied
- `theme.json` from the old theme's header (name, author, version, license, `screenshot.png` as the preview). Google Fonts used by the CSS are declared in `csp`
- the old `style.css` is loaded **after** the starter's, so its rules win

Then it prints what you have to check. Preview it with `theme dev` (or Theme Builder), fix what looks different, and set `[theme].active` to the new ID.

### Template Hierarchy

| WordPress                          | AegisCMS                    | Notes                                                                                                      |
| ---------------------------------- | --------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `header.php` + `footer.php`        | `templates/base.html`       | One skeleton for every page. Put the old header and footer markup here                                     |
| `index.php`, `home.php`            | `index.html`                | The post list                                                                                              |
| `front-page.php`                   | `front.html`                | Set `[theme].front_page = "template"`                                                                      |
| `single.php`, `single-post.php`    | `article.html`              | A post                                                                                                     |
| `page.php`                         | `page.html`                 | A static page                                                                                              |
| `category.php`, `archive.php`      | `category.html`             | A category's list                                                                                          |
| `tag.php`                          | `tag.html`                  | A tag's list                                                                                               |
| `search.php`                       | `search.html`               | Site search ([search.md](search.md))                                                                       |
| `404.php`                          | `404.html`                  |                                                                                                            |
| `comments.php`                     | `comments.html`             | The built-in comment section is used when the theme has none                                               |
| `sidebar.php`, widgets             | —                           | Not supported. Put fixed content in `base.html`                                                            |
| `functions.php`                    | —                           | Menus, widgets, image sizes and Customizer settings are not imported                                       |
| `template-parts/*.php`             | `{% include "name.html" %}` | Copy the markup into a template (the starter's `loop.html` is an example)                                  |
| `page-{slug}.php`, custom template | a page's `layout` field     | `layout` = `blank`, or the name of a theme template ([theme_authoring_guide.md](theme_authoring_guide.md)) |

### Template Tags and Variables

| WordPress                                              | AegisCMS                                                                                                  |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------- |
| `bloginfo('name')`                                     | `{{ site_title }}`                                                                                        |
| `bloginfo('description')`                              | `{{ site_description }}`                                                                                  |
| `home_url('/')`                                        | `{{ base_path }}/`                                                                                        |
| `get_stylesheet_uri()`, `get_template_directory_uri()` | `{{ base_path }}/theme/assets/…`                                                                          |
| `language_attributes()`                                | `lang="{{ locale }}"`                                                                                     |
| `wp_head()`                                            | `{{ extra_head \| safe }}` (and `{{ extra_css \| inline_style }}`)                                        |
| `wp_footer()`                                          | Nothing to add                                                                                            |
| `body_class()`                                         | Write the classes in `base.html` (the starter does: `single`, `page`, `home blog`, `archive`, `error404`) |
| `have_posts()` / `the_post()` loop                     | `{% for item in items %}…{% endfor %}`                                                                    |
| `the_title()`                                          | `{{ title }}` (one post) / `{{ item.title }}` (in a list)                                                 |
| `the_content()`                                        | `{{ content_html \| safe }}`                                                                              |
| `the_excerpt()`                                        | `{{ item.excerpt }}`                                                                                      |
| `the_permalink()`                                      | `{{ item.fields.url }}`                                                                                   |
| `get_the_date()`                                       | `{{ published_at \| date_short }}`                                                                        |
| `the_author()`                                         | `{{ author }}`                                                                                            |
| `the_category()`                                       | `{{ relations.category_id.title }}`, link `{{ base_path }}/category/{{ relations.category_id.slug }}`     |
| `the_tags()`                                           | `{% for tag in fields.tags %}{{ tag.name }}{{ tag.url }}{% endfor %}`                                     |
| `comments_template()`                                  | `{% if comments %}{% include "comments.html" %}{% endif %}`                                               |
| `the_posts_navigation()`, `paginate_links()`           | `pagination` (`prev_url`, `next_url`, `page`, `total_pages`)                                              |
| `wp_nav_menu()`                                        | Write the links in `base.html`                                                                            |
| `__()`, `_e()`, `esc_html__()`                         | Write the text directly, or use `{{ t("key") }}`                                                          |
| `is_front_page()`, `is_single()`, `is_page()`          | `content_type` (`index`, `article`, `page`, `error`, …)                                                   |

Rules the PHP themes did not have: write links with `base_path`, never put inline scripts or `style=` attributes in a template (the CSP blocks them), and output the body with `| safe` only for `content_html`. See the [Theme Authoring Guide](theme_authoring_guide.md).
