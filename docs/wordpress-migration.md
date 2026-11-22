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

| WordPress                            | AegisCMS                                                                                                                                                                          |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Posts                                | `article`, with the original publication date, excerpt, first category and tags                                                                                                   |
| Pages                                | `page`                                                                                                                                                                            |
| Categories                           | `category` content                                                                                                                                                                |
| Tags                                 | Tags. A tag whose slug is not ASCII (for example Japanese) gets the slug `wp-tag-<ID>`; the name is kept                                                                          |
| Media                                | Uploaded under `wordpress/` keeping the year and month folders; links in the content (including resized copies such as `-300x200`) are rewritten                                  |
| Authors                              | Users with the role `contributor`, created **disabled** with a random password. Enable them and set a password to let them log in                                                 |
| Drafts, pending, scheduled           | The same status                                                                                                                                                                   |
| Private and password-protected posts | Imported as **drafts**, so nothing private becomes public. They are listed in the report                                                                                          |
| Trash                                | Not imported                                                                                                                                                                      |
| Comments                             | Imported with their status (approved, pending, spam, trash), date and replies, only on published posts. No email is sent. Needs `[comments]` enabled for the content type         |
| Old URLs                             | 301 redirects from the old permalink to the new URL. A big site gets one pattern rule for the permalink structure (for example `/{p1}/{p2}/{slug}`) instead of thousands of rules |

Posts are converted to blocks: paragraphs, headings, lists, quotes, code, tables, images, separators and details. Things that cannot be converted exactly are kept as an HTML block and listed in the report.

## What Is Not Moved

- PHP themes and plugins. See the report for what the content used.
- Shortcodes other than `[caption]`, `[embed]` and `[gallery]`, page-builder content, WooCommerce products, form plugins, and custom post types.
- Featured images, parent categories and parent pages (the structure is flat), and SEO plugin fields.
- Pingbacks and trackbacks, comment authors that were logged-in users (they become plain names), and old URLs of the form `/?p=123` (a query on the home page cannot be redirected).
