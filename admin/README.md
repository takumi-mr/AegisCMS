# AegisCMS Admin Studio

**English** | [日本語](README.ja.md)

**AegisCMS Admin Studio** is the official admin SPA (Single Page Application) of AegisCMS, the next-generation secure CMS.  
It provides a modern admin interface for intuitively working with content management tied to declarative YAML schemas, an AST (abstract syntax tree)-based block editor, and extensibility through Wasm plugins.

> [!NOTE]
> The documents linked from this README (under `docs/`) are currently written in Japanese only.

---

## 🌟 Key Features

- **Zero-trust block editor**:
  - Add and edit headings (H1–H6), paragraphs, lists, code blocks (with syntax specification), images, tables (`TableBlockEditor.tsx`: adding and removing rows and columns, header rows, column alignment), table of contents, quotes, notes, collapsibles (Q&A), dividers, and custom HTML blocks (for block specifications, see [Body blocks](../docs/blocks.md)).
  - Custom blocks from enabled plugins (`GET /api/v1/blocks`) also line up in the toolbar and are edited in a form built from the input fields declared in the manifest (`PluginBlockEditor.tsx`) ([Body blocks 4](../docs/blocks.md#4-プラグインのブロック)).
  - In paragraphs, lists, table cells, and so on, you can use the inline formatting `**bold**`, `*italic*`, `` `code` ``, and `[link](URL)`.
  - Intuitive block reordering (Up / Down) and safe node deletion.
  - It edits the AST structure (`BlockNode`) directly, structurally preventing injection of raw malicious scripts.
- **Real-time HTML preview**:
  - The block being edited is rendered by the server's preview API (`POST /api/v1/preview`), showing the same HTML as when saved and displayed on the public page (including heading ids, plugin AST transformations, and link and image rewriting). It updates each time input pauses for 300ms, and shows the previous result dimmed while waiting for the update.
  - The **Themed Page** tab shows the whole page with the theme's templates and the `on-render-page` hook applied in an `iframe` (its `src` is the URL of a page the server placed, with `sandbox="allow-scripts"`; plugin scripts such as Mermaid rendering run as on the public page, but cannot enter the Admin Studio origin). Changes to the title, slug, and category are reflected too.
  - The rendered result can be switched with tabs between a preview and an HTML source view. The "HTML output" tab shows the body HTML in an `iframe` (a document the server placed; `body_frame_key`, `body_url`) (because Admin Studio's CSP does not allow inline styles; [csp.md 1.1](../docs/csp.md#11-admin-studio-の-style-src)).
  - When an article or page has "who can view" (membership level) set, the **Viewer** selector above the preview (staff, guest, and per-member-level) lets you check what that reader sees (the part before the divider and the notice). It does not use a real member session; the server renders it with the same decision as the public page (`viewer` in `POST /api/v1/preview`; [members.md 15.6](../docs/members.md#156-プレビューとの関係)).
  - To the right of the tabs are **Full screen** (return with Esc) and **Open in a new window** (opens the same URL as the `iframe` as is; it is the page at that moment and does not follow updates). The page is returned with `Content-Security-Policy: sandbox allow-scripts` (an opaque origin), so even in a new window it cannot reach the session token.
- **Scheduled publishing and unpublishing**: Choose "Scheduled" as the status in the editor or product form, enter a publication date and time (`datetime-local`), and save, and the server publishes at that time. You can also give published or scheduled content an unpublish date and time (`unpublish_at`) for time-limited publication. In lists, besides the status badge, an unpublish-scheduled icon (with the date and time in a tooltip) is shown ([Scheduled publishing and unpublishing](../docs/theme_and_publishing_guide.md#予約公開スケジュール公開)).
- **Publishing schedule calendar (Calendar)**: From the "Calendar" tab in the header (`ScheduleCalendar.tsx`), view the publication schedule, unpublish schedule, and published items of all content types in monthly calendar and agenda formats. It supports filtering by content type and schedule kind, quick editing and clearing of unpublish times from the detail modal, and jumping directly to the editor.
- **Analytics**: In the "Analytics" tab in the header (`AnalyticsView.tsx`), shown only when `[analytics].enabled = true`, see the trends in page views and visitors over the past 7 / 30 / 90 days (as graphs and tables; the graphs can be read with the arrow keys too), and the most viewed pages, referrers, and devices ([docs/analytics.md](../docs/analytics.md)).
- **Contact (Inbox)**: In the "Contact" tab in the header (`ContactInbox.tsx`), shown to editor and above when `[contact].enabled = true`, list messages arriving from the contact form by new / read / handled / spam, view details, reply by email, change status in bulk, and delete. You can place the "Contact form" block (`core/contact-form`, `[CONTACT]` in Markdown) in the body ([docs/contact.md](../docs/contact.md)).
- **Markdown editing & pasting**:
  - Switch the editing method with **Blocks / Markdown** at the top of the editor. Blocks and Markdown convert to each other (`blocksToMarkdown` / `markdownToBlocks` in `utils/markdown.ts`; headings, paragraphs, code, images, lists, tables, table of contents (`[TOC]`), quotes, notes (`> [!NOTE]`, `:::note`), collapsibles (`:::details`), dividers, and plugin blocks (`::: <name> {attributes JSON}`)), and whichever you edit in, the preview and saving are done from the same blocks.
  - Markdown editor (`MarkdownEditor.tsx`): a formatting insertion toolbar, indentation with Tab, saving with Ctrl+S, and display of line, character, and block counts.
  - Pasting Markdown into the block editor converts it to blocks and adds them. **Paste Markdown** (`PasteMarkdownModal.tsx`) reads the clipboard, lets you check the breakdown of the conversion result, and then choose to append at the end or replace everything.
- **Products / Orders**: Create and edit products (price, inventory, SKU, images); search the product list (name, slug, SKU, summary), filter by status, and page through it 25 at a time (the list fetches summaries only, and the whole item is fetched when you open it); and list orders, view details, and change status (payment confirmation, shipping, cancellation). The tabs are shown only when e-commerce is enabled (`[commerce].enabled`) ([E-commerce](../docs/commerce.md#33-admin-studio)).
- **Comments**: Approve, unapprove, mark as spam, trash, and delete (one at a time or in bulk) readers' comments, edit the body, reply, stop and resume accepting comments per content, and switch the approval workflow (`admin`). The tab is shown only when comments are enabled (`[comments].enabled`; `GET /api/v1/comments/settings` is not 404) and the role is `editor` or above. The URL `#comments` (the link in notification emails) opens this tab ([Comments](../docs/comments.md#4-admin-studio)).
- **Site (Settings › Site)**: Change the public site's name and favicon (`GET` / `PUT /api/v1/site`, `admin` only; `SiteManager.tsx`). Choose the favicon from the media library, upload it, or enter a URL. Saving it empty returns to the value in the config file (`[theme]`) ([Themes and publishing](../docs/theme_and_publishing_guide.md#サイト名と-faviconadmin-studio)).
- **System operation settings (Settings › System)**: Dynamically change, without restarting the server, the maintenance mode toggle (editing the notice text), the asset upload limit (MB), enabling CDN/browser caching and the TTL, stopping and resuming user self-registration, the lockout count for consecutive login failures, and stopping and resuming comment and forum post acceptance (`GET` / `PUT /api/v1/system/settings`, `admin` only; `SystemManager.tsx`; [System settings](../docs/configuration.md#53-システム運用設定-system)). It has a button to reset to defaults.
- **Notifications (Settings › Notifications)**: The delivery state (channels, format, and where the outbox is stored), the list of pending and failed notifications (resend, discard), test sends with sample values (showing the result per channel), editing templates (subject, body, and HTML per kind and recipient, and shared parts) with previews of the unsaved content (HTML in an `iframe` `srcdoc` with `sandbox=""` so scripts do not run), and restoring defaults. `admin` only (`NotificationManager.tsx`; [Notifications 6](../docs/notifications.md#6-admin-studio-から管理する)).
- **Themes (Settings › Themes)**: Cards of installed themes (preview image, tags, author), one-click switching (`PUT /api/v1/themes/active`, `admin`), and checking the public site. The marketplace tab is a teaser for a future feature (ZIP installation is not implemented).
- **User management (Settings › Users) & profile**:
  - `admin` can list, create, edit, change roles of, disable, reset passwords of, and delete users (`UserManager.tsx`, [Authentication 3](../docs/authentication.md#3-ユーザー管理-api)).
  - **Invite user**: Provides a modal (`InviteUserModal`) where an administrator specifies an email address and role and sends an invitation email ([Invitation links](../docs/authentication.md#10-ユーザー招待リンク)).
  - **Pending approval status**: Accounts awaiting approval after self-registration (`pending_approval: true`) show a dedicated "Pending approval" badge, and can be "approved and enabled" in one click from the edit modal.
  - From the avatar + display name in the header, **My profile** (`ProfileModal.tsx`): change the avatar image, display name, email address, and password (the password field is not shown for SSO users). When there is no avatar or it cannot be read, initials are shown (`ui/Avatar.tsx`).
- **Premium glassmorphism UI**:
  - Built with zero-runtime CSS Modules and CSS variables (design system tokens), without depending on frameworks such as TailwindCSS.
  - An eye-friendly, highly legible dark theme, beautiful frosted-glass effects (glassmorphism), and refined micro-interactions.
  - Typography uses `Plus Jakarta Sans` / `Outfit` / `JetBrains Mono`.
- **Image upload**:
  - Drag and drop onto an image block, or click to choose a file, and it uploads to `/api/v1/storage/upload` and the URL is reflected into the block automatically (if alt is empty, it is filled in from the file name).
  - Files other than images, and files exceeding the server's limit (`[storage].max_upload_mb`), show an error.
- **Media library**:
  - With **Choose from media library** on an image block, uploaded images (`GET /api/v1/storage/objects?content_type=image/`) are shown as a thumbnail list, newest first. It supports file name search and loading more (24 at a time).
  - Choosing an image reflects its URL into the block (if alt is empty, it is filled in from the file name). You can reuse the same image without uploading it again.
- **Category selection & management**:
  - In the article editor, choose a category (the `category_id` relation) from a dropdown. Opening an existing article restores the selection.
  - The category list (with colors and descriptions) and creation of new ones (automatic slug generation, color specification) are in **Settings › Categories**. They can also be opened from the editor's **Manage** button.
  - A category badge is shown in the sidebar's article list.
- **Search, filtering, and pagination of the content list (sidebar)**: A search box for title, slug, and summary (`excerpt`) (searches when input pauses for 300ms, or immediately on Enter), filtering by category (All / Uncategorized / each category; articles only) and status (Draft / Scheduled / Published), and paging 20 per page. Searching, filtering, and paging are done on the server (`GET /api/v1/contents/{type}?view=summary&q=&category=&status=&limit=&offset=`), and the list fetches only summaries (title, slug, status, category, summary; without blocks or rendered HTML). Selecting an article fetches that whole item and opens it in the editor. Switching the kind (article / static page) resets the search and filters.
- **Newsletter (Settings › Newsletter, `admin`)**: Counts per status (subscribed, awaiting confirmation, stopped); choose a published article and send it to subscribers (edit the summary, send a test to yourself, confirm before sending, and confirm before resending an already-sent article); the subscriber list (filter by status, search by email address, delete, and CSV export); and the send history (`NewsletterManager.tsx`; [Newsletter](../docs/newsletter.md)). When `[newsletter].enabled = false`, it shows a notice that server configuration is required.
- **Notices (Settings › Notices, `admin`)**: The list of notices shown at the top of all pages (status marks: showing, scheduled, ended, disabled) and adding, editing, and deleting them (`NoticeManager.tsx`). The period is entered in the browser's local time and sent in UTC. The server's reasons are in English, so text in the screen's language is shown from `code` and `field` ([Notice banners](../docs/notices.md)).
- **Redirects (Settings › Redirects, `admin`)**: The list of old URL → new URL rules (newest first, 25 at a time, search by path) and adding, editing, and deleting them (`RedirectManager.tsx`). The kinds are 301 (permanent) / 302 (temporary). Rules added automatically when a slug was changed and published get an "auto" mark. The server's reasons are in English, so text in the screen's language is shown from `code` (such as `duplicate_redirect`) ([Redirects](../docs/redirects.md)).
- **Tags (Settings › Tags, editor, product editing)**: The tag list (by name, 25 at a time, search by name or slug, number of content items attached) and creating, editing, and deleting (creating and changing require `editor` or above). The article and page editors and product editing have a "Tags" field (`TagPicker`) where you type a name to find and attach an existing tag, or create one on the spot and attach it. Tags are saved when the content is saved (`PUT /api/v1/contents/{type}/{id}/tags`). When the server has `[tags].enabled = false`, the Settings item and the editor field are not shown ([Tags](../docs/tags.md)).
- **Settings screen**:
  - Site-wide management (site name and favicon, SEO, notifications, themes, plugins, categories, users, audit logs) is gathered under **Settings** in the header, switched with the menu on the left. The header tabs are Editor / Products / Orders / Comments / Settings (Products / Orders when e-commerce is enabled and the role is `editor` or above; Comments when comments are enabled and the role is `editor` or above).
  - Items are shown according to role: Themes, Plugins, and Categories for everyone (creating categories requires `editor` or above); Site, Users, and Audit Logs for `admin` only.
- **Audit log viewer (Settings › Audit Logs)**:
  - Lists audit logs (`GET /api/v1/audit-logs`). It supports filtering by action / outcome / actor / resource / period, paging, and expanding the event details (JSON).
  - Shown only when logged in with the `admin` role (for details, see the [Audit log reference](../docs/audit.md#6-admin-studio-の閲覧画面)).
- **Login & roles**:
  - The login screen is shown at startup, and you log in to `POST /api/v1/auth/session` with a username / password. The issued token is kept in `sessionStorage` until the tab is closed and attached to every API call as `Authorization: Bearer` (the logged-in state survives reloads).
  - The header shows the display name and role, and the logout button discards the session. When the API returns 401 (expired or revoked), it returns to the login screen.
  - If account registration (`[auth.registration]`) is enabled on the server, "Create an account" is shown on the login screen (`RegisterForm.tsx`). Opening the confirmation email's link (`#verify-email=<token>`) removes the token from the address bar, creates the account, and shows the result (can log in / pending approval / link invalid) ([Authentication 7](../docs/authentication.md#7-アカウント登録)).
  - **Password reset (`PasswordResetForms.tsx`)**: When `[auth.password_reset]` is enabled, submit your email address from "Forgot your password?" on the login screen, and set a new password from the link in the reset email that arrives (`#reset-password=<token>`) ([Authentication 9](../docs/authentication.md#9-パスワードの再設定)).
  - **Accepting an invitation (`AcceptInviteForm.tsx`)**: A user invited by an administrator opens the link in the email (`#invite=<token>`), checks the invitation information (email, role), and then sets a username and password to open an immediately active account ([Authentication 10](../docs/authentication.md#10-ユーザー招待リンク)).
  - Operations are restricted by role: for `viewer`, Save, image upload, and category creation are disabled, and Users / Audit Logs in the settings screen are shown to `admin` only (the server validates the same permissions; [Authentication & user management](../docs/authentication.md)).
  - A `contributor` can save only their own drafts and items awaiting review, and can choose only "Draft" and "Awaiting review" as the status (`canWrite` / `canChangeContent` in `utils/roles.ts`). Published items and other people's items are read-only, and version restore is not shown. The shop, comments, creating categories, and so on require `editor` or above (`canEdit`).
  - **Review**: When saving with a status change, a note field (`review_note`) appears. Editors see the number of items awaiting review above the list, and pressing it filters the list. `#review` (the link in the review request notification email) opens the list filtered to items awaiting review ([Review](../docs/theme_and_publishing_guide.md#レビュー寄稿者の下書きを承認して公開する)).
- **Multilingual (English / Japanese)**:
  - Switch with the language menu in the header and on the login screen. The chosen language is stored in the browser, and the first time it is shown in the browser's language (Japanese if Japanese, otherwise English). Date and amount formats follow the language too.
  - The text is [i18next](https://www.i18next.com/) JSON (`src/locales/<language>/translation.json`), which can be handed straight to translation services ([Internationalization](#-internationalization-i18n)).
- **Color theme (light / dark / follow system)**:
  - Switch with the theme selector in the header (the menu on smartphones) and on the login screen (`ThemeSelect.tsx`). The chosen theme is stored in the browser (`aegis.admin.theme` in `localStorage`), and the first time it follows the OS setting (`prefers-color-scheme`). "Follow system" also follows OS switches. It becomes light only when the OS asks for light, and browsers that cannot read the setting show dark as before.
  - Before rendering, `public/theme-init.js` sets `<html data-theme>`, so dark never flashes briefly at first when light is chosen ([Color theme design](#-design-notes)).
- **Transparent integration with the Core Engine**:
  - Real-time sync with the schema list (`/api/v1/schemas`) and with article listing, details, and creation (`/api/v1/contents/{content_type}`).
  - Connection status display through the backend server's health check (`/api/v1/health`).
  - The production build output (`dist/`) is served automatically on a single port (8080) by the Rust core server (`aegis-server`).

---

## 🏗️ Tech Stack

| Category | Technology | Rationale |
| :--- | :--- | :--- |
| **Framework** | React 19 + TypeScript | Type safety and a robust component-oriented UI |
| **Build tool** | Vite 6 | Fast HMR (Hot Module Replacement) and minimal bundle size |
| **Styling** | Vanilla CSS (CSS Variables) | High design flexibility, framework independence, zero runtime |
| **Icons** | Lucide React | A lightweight and consistent modern icon set |
| **Internationalization** | i18next + react-i18next | A JSON format that translation services (Crowdin, Lokalise, etc.) can handle, with plurals and interpolation |

---

## 📁 Directory Layout

```text
admin/
├── src/
│   ├── api/
│   │   ├── client.ts       # REST API gateway client (attaches Bearer, notifies on 401)
│   │   └── session.ts      # Keeping the session token (sessionStorage)
│   ├── components/         # Screen-level components (next to each *.tsx, a same-named *.module.css and a Storybook *.stories.tsx)
│   │   ├── ui/             # Shared components
│   │   │   ├── Button.tsx  # Button (primary / secondary / tool / icon)
│   │   │   ├── Field.tsx   # Labeled input Field and the side-by-side FieldRow
│   │   │   ├── Modal.tsx   # Modal (closes with Escape or an outside click, focus trap)
│   │   │   ├── LanguageSelect.tsx # Language switcher
│   │   │   ├── RoleLabel.tsx # Display name of a role
│   │   │   ├── Avatar.tsx  # Avatar (initials if there is no image)
│   │   │   └── BrandMark.tsx # Logo mark
│   │   ├── Header.tsx      # Header, screen switching, server status, logged-in user
│   │   ├── LoginView.tsx   # Login screen
│   │   ├── RegisterForm.tsx # Account registration (from the login screen)
│   │   ├── PasswordResetForms.tsx # Password reset request and confirmation forms
│   │   ├── AcceptInviteForm.tsx # Accepting an invitation link and initial account setup
│   │   ├── SettingsView.tsx # Settings screen (the menu on the left switches the screens below)
│   │   ├── SiteManager.tsx # The public site's name and favicon
│   │   ├── ThemeManager.tsx # Theme list and switching
│   │   ├── UserManager.tsx # User management
│   │   ├── ProfileModal.tsx # Your own profile (AvatarPicker.tsx chooses the avatar)
│   │   ├── ProductManager.tsx # Products
│   │   ├── OrderManager.tsx # Orders
│   │   ├── AnalyticsView.tsx # Analytics dashboard
│   │   ├── ContactInbox.tsx # Contact inbox
│   │   ├── CommentManager.tsx # Comment moderation, replies, and approval settings
│   │   ├── AuditLogView.tsx # Audit log viewer
│   │   ├── CategoryManager.tsx # Category list and creation
│   │   ├── MediaLibrary.tsx # Modal for choosing uploaded images
│   │   ├── ImageUploader.tsx # Upload field for image blocks (drag and drop)
│   │   ├── ContentList.tsx # Article and content list sidebar
│   │   ├── BlockItem.tsx   # Editors for the various AST blocks
│   │   ├── MarkdownEditor.tsx # Markdown editing mode
│   │   ├── PasteMarkdownModal.tsx # Pasting Markdown
│   │   └── Preview.tsx     # Real-time HTML / source code preview
│   ├── i18n/               # i18next initialization, language detection and storage (index.ts), key types (i18next.d.ts)
│   ├── theme/              # Saving, applying, and OS-following of the color theme (index.ts), design token checks (tokens.test.ts)
│   ├── locales/            # Translations: en/translation.json (source text), ja/translation.json
│   ├── utils/              # Category, media, and role helpers, date formatting (format.ts), helpers such as cx
│   ├── types/
│   │   ├── index.ts        # AST node, schema, and content type definitions
│   │   └── schemas.ts      # Types generated automatically from schemas (task codegen:ts)
│   ├── App.tsx             # Main application component
│   ├── App.module.css
│   ├── index.css           # Global: design tokens (CSS variables), reset, and basic elements only
│   └── main.tsx            # Entry point
├── .storybook/             # Storybook configuration (main.ts, preview.tsx) and API mocks (mockApi.ts)
├── e2e/                    # Playwright E2E tests (*.spec.ts) and starting a throwaway server (start-server.mjs)
├── scripts/check-locales.mjs # Verifies that all languages have the same keys as English (run by npm run lint)
├── index.html              # HTML template
├── package.json
├── playwright.config.ts
└── vite.config.ts
```

### How to Write Styles

- Write a component's styles in the neighboring `*.module.css` (CSS Modules), import it with `import styles from './X.module.css'`, and reference it as `className={styles.foo}`. Class names are scoped per file at build time, so they do not collide with other components.
- For colors, spacing, border radius, fonts, and so on, use the CSS variables in `index.css` (such as `var(--bg-surface)`). Use inline `style={{...}}` only for values decided at runtime, like a category color.
- For buttons, input fields, and modals, use the shared components in `components/ui/`. To adjust a shared component's appearance from a parent, pass a `className` and specify it in the parent's module with a descendant selector such as `.parent .child` (so that it can be overridden without depending on CSS load order).
- To combine multiple classes, use `cx(styles.a, active && styles.b)` from `utils/cx.ts`.

---

## 🌐 Internationalization (i18n)

Screen text is not written directly in code; it is referenced by keys in `src/locales/<language>/translation.json`. English (`en`) is the source, and other languages are translations of it.

```tsx
import { useTranslation } from 'react-i18next';

const { t } = useTranslation();
<span>{t('orders.markPaid')}</span>                        // Mark as paid
t('orders.updated', { number: order.number, status })     // Interpolation: "Order {{number}}: {{status}}"
t('media.inUse', { count: 3 })                             // Plurals: media.inUse_one / media.inUse_other
<Trans i18nKey="profile.signedInAs" values={{ username }} components={{ code: <code /> }} />
```

- **Keys are type-checked** (`src/i18n/i18next.d.ts`). Passing a key that is not in the English JSON to `t()` makes `npm run build` (`tsc`) fail.
- **Missing translations are caught by lint**. At the end of `npm run lint`, `scripts/check-locales.mjs` verifies that each language's keys match English (missing keys, extra keys, empty values). Plural forms differ by language (English has `_one` / `_other`, Japanese has only `_other`), so they are compared together as a single key.
- Outside components (such as API client error messages), use `i18n.t()` from `import i18n from '../i18n'`. Error messages inside `useEffect` should also use `i18n.t()` (putting `t` in the dependency array would reload when the language is switched).
- For dates, use `formatDate` / `formatDateTime` in `utils/format.ts`, and for amounts, `formatMoney` in `utils/money.ts`, and they use the format of the displayed language.
- Error messages returned by the server (the API's `error`) are shown in English as they are. The screen rephrases only some of them, such as login failures, insufficient permissions, and account registration failures (the API's `code`).

### Adding a Language

1. Create `src/locales/<language code>/translation.json` from the English file and translate it
2. Add it to `LANGUAGES` and `resources` in `src/i18n/index.ts`
3. Run `npm run lint` to check that the keys match

### Integrating with Translation Services

The files are i18next JSON (v4, nested keys), so they can be read and written as is in the "i18next JSON" format of Crowdin, Lokalise, Weblate, and others. For example, with Crowdin:

```yaml
# crowdin.yml (repository root)
files:
  - source: /admin/src/locales/en/translation.json
    translation: /admin/src/locales/%two_letters_code%/translation.json
```

Format files returned from a translation service with `npm run format`, then run them through `npm run lint` / `npm run build`.

### i18n Details

- The language is `aegis.admin.language` in `localStorage`, or else `navigator.languages` (`en` for anything other than `ja`). `<html lang>` is switched too.
- Login screen notices (expired, logged out) and SSO errors are held as reason codes instead of strings and translated when displayed (so they follow a language switch).
- Audit log action names, theme descriptions (`theme.json`), and item names that plugins attach (such as `Coupon WELCOME10`) are data and are not translated.
- Because Japanese labels are long, header elements are `nowrap`, and the badge is hidden below 1680px width and the gateway text below 1480px (the status dot and tooltip remain).

---

## 🧩 Design Notes

- **Color theme**: All colors are decided by the design tokens (CSS variables) in `src/index.css`. `:root` is dark (the default) and `:root[data-theme='light']` is light, and `<html data-theme>` is set by `public/theme-init.js` (before rendering; an external file so as not to rely on CSP `unsafe-inline`) and `src/theme` (later switches and OS following). Both decide the same way, and a unit test actually runs `theme-init.js` and compares it with `resolveTheme`. When writing component CSS, follow these rules (`src/theme/tokens.test.ts` checks them).
  - Write colors with tokens. Do not write "translucent white" such as `rgba(255, 255, 255, 0.08)`; use `rgb(var(--overlay-rgb) / 0.08)` (white in dark, slate in light). For recessed surfaces, use `--inset-soft` / `--inset` / `--inset-strong`.
  - For pale text on accent colors, use `--text-rose` / `--text-amber` / `--text-teal` / `--text-indigo-soft` / `--text-blue` (pale in dark, deep in light). Only white text on filled buttons and badges stays `#fff`.
  - Define the tokens you use in `index.css` (an undefined `var(--x, #fallback)` is pinned to the fallback color and does not follow the theme). When you add a token for dark, add a value for light too.
  - Colors verified against the background, like chart series colors, are placed in the component's CSS, and the light value is overridden with `:global(:root[data-theme='light'])` (`AnalyticsView.module.css`).
  - In Storybook, you can view light / dark by switching **Theme** in the toolbar (story tests run in dark).
- **API target**: `SERVER_URL` in `src/api/client.ts` (origin + `[server].base_path`) is decided in the order: `VITE_AEGIS_SERVER_URL` at build time → `http://127.0.0.1:8080` for the dev server → for production builds, the bundle's URL (`../../` from `{server}/admin/assets/*.js`). There is no need to rebuild per domain or base path. Root-relative URLs that the server returns (such as avatars) are displayed with the origin prepended by `serverAssetUrl`.
- **Editing extra schema fields**: Fields without a dedicated input are shown, from the definition in `GET /api/v1/schemas`, with a generic input per type (`ExtraFieldsEditor`: string / text / integer / boolean / timestamp / relation (enter the target ID directly) / jsonb (enter JSON directly)). The targets are decided by `extraFieldDefs` in `utils/extraFields.ts` (excluding `category_id` and fields with a dedicated UI listed in `DEDICATED_FIELDS` — an article's `category_id`, a static page's `layout`, a product's price and inventory, and so on). The server's update replaces all fields, so extra fields are sent back as they are on every save even if not edited (so values set through the API are not lost by saving in the admin screen). An empty value is `null` if `nullable`, and otherwise not sent. Extra fields are assumed not to be used in core logic, and are not reflected in list search and filtering. When you add a dedicated input for a field, add it to `DEDICATED_FIELDS` too.
- **Content types**: The generated `types/schemas.ts` is the shape of the content API itself. Custom fields are `fields` in both responses and writes (such as `ArticleFields` per type), and resolved relations are `relations`. Write payloads (such as `CreateArticlePayload`) put standard items at the top level and custom fields in `fields`. Screens use `ContentItem` (`BaseContentItem`) from `types/index.ts`. The editor's list fetches summaries (`ContentSummary`, without blocks or rendered HTML) one page at a time with `listContentSummaries`, and small lists like categories fetch all summaries with `getAllContentSummaries`. The listing of whole data (with blocks and rendered HTML) is not used from screens because the server renders the blocks of each item (only the single item being opened is fetched with `getContent`).
- **Saving**: An open article is saved with `PUT` (update), and a new one with `POST`.
- **Themed Page preview**: When `POST /api/v1/preview` is given a `frame_key` (a 32-digit hexadecimal random value the editor creates; one per preview), the server places `page_html` in the cache (for 1 hour; the same name is overwritten) and returns `page_url` (`/preview/frame/{key}?v=...`). The `iframe` loads this as its `src`. **Do not use `srcdoc`**: a `srcdoc` document inherits Admin Studio's CSP (`script-src 'self'`, no `unsafe-inline`), so the themes' and plugins' inline scripts do not run. Loaded from a URL, it runs with the same CSP as the public page. Only people who know the name can read it (no authentication; a 128-bit random value). Do not add `allow-same-origin` to `sandbox` (if added, scripts output by themes and plugins could read the session token in `sessionStorage`). The server also adds `sandbox allow-scripts` through the CSP. Without it, the document's origin becomes opaque (`null`), and touching storage such as `localStorage` throws an exception (`cart.js` catches it with try/catch and keeps the data in memory). Ordinary `<script>` loads from CDNs work, but `type="module"` and loads that need CORS (such as fonts with `crossorigin`) need `Access-Control-Allow-Origin` on the server side. Theme assets and uploaded images can be loaded root-relative as is, because the page is served by the server itself.
- **Screen height and scrolling**: The root `.app` is `height: 100vh; overflow: hidden`. Full-width screens (audit logs, themes, etc.) put a `flex: 1; overflow-y: auto` wrapper directly below, leaving bottom spacing so they scroll.
- **Settings screen (`SettingsView.tsx`)**: Each item displays the existing screen component as is (`SiteManager` / `SeoManager` / `NotificationManager` / `ThemeManager` / `PluginManager` / `CategoryManager` / `UserManager` / `AuditLogView`). To add an item, add to `SECTIONS` and the translation of `settings.sections.<id>`. The selected item is held by `App` (so that the editor's **Manage** can open `categories`). If an item hidden by role was selected, the first item is shown. The editor's state is also in `App`, so what is being edited does not disappear when you move to the settings screen and come back.
- **Mobile and responsive support**: On narrow devices with a screen width of 900px or less, the header becomes compact, and the tabs, connection status, language switch, and account actions are integrated into a hamburger menu. The editor screen switches from the fixed 3 columns (300px / flexible / 480px) to pane-switching tabs (article list / edit / preview), and selecting an article from the list moves automatically to the edit pane. With wrapping field rows, horizontal scrolling for calendars and tables, and single-column admin screens, the UI does not break on smartphones and tablets and can be operated intuitively.

---

## 🚀 Development & Build Steps

### 1. Install Dependencies

```bash
npm install
```

### 2. Start the Dev Server (Standalone Development)

```bash
npm run dev
```

By default `http://localhost:5173/` starts.  
* The dev server connects to the core server at `http://127.0.0.1:8080` (`cargo run -p aegis-server -- run`) (CORS is already allowed). To connect to a server elsewhere, specify it like `VITE_AEGIS_SERVER_URL=https://example.com/cms npm run dev`.

### 3. Type Check & Production Build

```bash
npm run build
```

After running the TypeScript type check (`tsc -b`), optimized static assets are written to `dist/`.  
The generated `dist/` is loaded automatically when the core server starts and served at `/admin/`. Assets are referenced by relative paths (`base: './'`) and the API target is derived from the bundle's URL (`{server}/admin/assets/*.js`), so it works without rebuilding behind a reverse proxy, on any subdomain, and under a `[server].base_path` subpath. To serve Admin Studio alone from a different place (such as `admin.example.com`), specify the AegisCMS server's URL with `VITE_AEGIS_SERVER_URL` at build time ([docs/configuration.md](../docs/configuration.md#43-admin-studio-の接続先)).

### 4. Build Preview

```bash
npm run preview
```

### 5. Static Analysis (oxlint)

```bash
npm run lint   # oxlint --deny-warnings (fails on even one warning; also run in CI)
```

The rules are in `.oxlintrc.json`. Like Rust's Clippy (`-D warnings`), it is a zero-warning policy.

### 6. E2E Tests (Playwright) and Storybook

```bash
npm run test:e2e         # builds the server and Admin Studio, and tests against a throwaway server on port 18080
npm run storybook        # http://localhost:6006
npm run build-storybook  # storybook-static/
```

The first time, install the browser with `npx playwright install chromium`. For details, see [Development guide 8](../docs/development.md#8-admin-studio-の-e2e-テストと-storybook).

### 7. Code Formatting (Prettier)

```bash
npm run format        # apply formatting
npm run format:check  # verify it is formatted (also run in CI)
```

The settings are in `.prettierrc.json` (single quotes, 100 characters per line, ES5 trailing commas). Indentation follows `.editorconfig` (2 spaces). The generated file `src/types/schemas.ts` and Markdown are excluded (`.prettierignore`).

**Format on save in VS Code**: The `.vscode/settings.json` at the repository root is already set up to format TS / TSX / JS / CSS / HTML / JSON with Prettier on save.

1. Install the recommended extension **Prettier - Code formatter** (`esbenp.prettier-vscode`) (it is registered in `.vscode/extensions.json`, so it is shown as recommended when you open the folder)
2. `npm install` (Prettier from `admin/node_modules` is used, so the version is pinned)
3. Open the repository root (`AegisCMS/`) in VS Code

Because `prettier.requireConfig` is enabled, files outside `admin/` that have no Prettier config (such as JSON in `config/`) are not formatted.

---

## 🔗 Related Links

- [AegisCMS overall design document (README.md)](../README.md)
- [Agent operating conventions (AGENTS.md)](../AGENTS.md)
- [Progress & TODO list (TODO.md)](../TODO.md)
