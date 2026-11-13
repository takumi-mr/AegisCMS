[← Back to the documentation index](README.md)

**English** | [日本語](csp.ja.md)

# Content-Security-Policy (CSP) Policy

The current state of the CSP for the public site and Admin Studio, and the design for making public pages a Strict CSP (which does not allow inline scripts).

> [!NOTE]
> Everything is implemented (how the work was split is in §7; fonts are in §8). **The default is `strict_csp = "on"` (a breaking change)**. To return to the previous behavior, use `"off"`.

---

## 1. Current State (Implemented)

| Target                                                         | `script-src`                                                    | Notes                                                                                                                                                                    |
| -------------------------------------------------------------- | --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Public pages and API                                           | `'self' 'unsafe-inline'`                                        | `'unsafe-eval'` is not included (no feature uses `eval`). Hosts declared by plugins (`capabilities.csp.script_domains`, etc.) and CAPTCHA hosts are added                |
| Admin Studio (`/admin/`)                                       | `'self'`                                                        | No `unsafe-inline`. `style-src` is also only `'self'` (§1.1)                                                                                                             |
| Preview of theme-applied pages                                 | Same as public pages                                            | `GET /api/v1/preview/frame/{key}` returns it with `sandbox allow-scripts` added. When Strict, the hashes collected when the page was placed are added to the CSP too     |
| Public pages (`[security].strict_csp = "on"`; **the default**) | `'self'` + hashes of the page's inline scripts + declared hosts | No `'unsafe-inline'` or `'unsafe-eval'` (§6). `style-src` is `'self'` + hashes of the page's `<style>` (no `unsafe-inline`; `style=""` attributes are stopped too; §6.1) |

- The default values are `headers::DEFAULT_CONTENT_SECURITY_POLICY` (`aegis_core::csp::DEFAULT_PUBLIC_CSP`; **it has no `'unsafe-inline'`**) and `ADMIN_CONTENT_SECURITY_POLICY`. Only when `strict_csp` is `off` / `report-only`, `aegis_core::csp::with_legacy_inline` adds `'unsafe-inline'` to `script-src` and `style-src` and enforces it (the previous behavior). When an administrator has overridden the CSP in "Settings › Headers", that value is used as is.
- `admin/e2e/csp.spec.ts` runs Admin Studio and the main public pages with the default CSP and detects blocked scripts and script errors.
- For the detailed background, see [architecture.md "Default defense header values"](architecture.md).

---

### 1.1 Admin Studio's `style-src`

Admin Studio does not use inline styles (`<style>` elements or `style` attributes). React's `style` is set through the CSSOM (`element.style`), so it does not hit `style-src`. So `style-src` was also made `'self'` only.

- **The 3 places that display HTML are not inserted directly into the screen but shown in a separate document (`iframe`) that the server placed**: the body HTML (the "HTML output" tab; plugin blocks have their own `<style>`), the theme-applied page, and the notification email preview (emails assume inline styles). `srcdoc` and direct insertion would inherit Admin Studio's CSP and block these styles.
- The server returns them at `GET /api/v1/preview/frame/{key}` (the `frame_key` and `body_frame_key` of `POST /api/v1/preview`, and the `frame_key` of `POST /api/v1/notifications/preview`). It adds `sandbox allow-scripts` to the same CSP as public pages, so even scripts inside cannot reach the admin session. The body document gets minimal styles for readability.
- A by-product: body HTML is no longer put into the admin origin's DOM (`dangerouslySetInnerHTML` was dropped).
- The Google Fonts (`fonts.googleapis.com`) that Admin Studio had been loading were being blocked by the CSP and not loading anyway, so they were removed from `index.html` (fonts fall back to system fonts).

## 2. Why `unsafe-inline` Remained on Public Pages (Removed in §6)

Because some things output inline `<script>` and `onclick` directly into the HTML.

- The `extra_head` of the official plugins (`page-toc`, `back-to-top`, `share-buttons`, `lightbox`, `external-links`, `code-lines`, `code-enhancer`, `mermaid`, `ads`, `clarity`)
- The built-in templates (the comment form's script, `onclick`) and the setup screen
- Third-party themes

With `'unsafe-inline'`, scripts that slip through sanitization also run. Sanitization is the only defense.

---

## 3. Goal (Design)

Make a public page's `script-src` allow **only scripts from trustworthy sources**.

```
script-src 'self' 'sha256-<hash of each page's inline script>' https://<hosts declared in the manifest>
```

### 3.1 Why Hashes Instead of Nonces

- A nonce needs a different value for every request. Public pages have CDNs and reverse proxies put the HTML in shared caches per response (`[cdn]` adds `Cache-Control: public` and `Cache-Tag`). The same nonce would be in the cached response's header and body and reused for many visitors, so the nonce would no longer be unpredictable and would give no protection.
- A hash is **decided by the page's content**, so it does not change when cached. As long as the header and body are in the same response, it always matches.
- `'strict-dynamic'` is not used. If used, the browser ignores `'self'` and host specifications and trusts child scripts loaded by a nonce'd script without checking the host. We keep the browser enforcing the `script_domains` declared in the manifest (dynamic loading, such as Clarity's startup code loading `www.clarity.ms`, passes too if the host is declared).

### 3.2 Compute Hashes Only for "Trustworthy Sources"

**The approach of hashing every `<script>` in the final HTML gives no protection.** Because a script an attacker inserted would be hashed and allowed along with the rest. Hashes are computed **at the time of output**, from the following sources.

1. **Plugins**: inline scripts received through structured injection (§4)
2. **Templates**: the built-in templates make inline scripts into external files (`'self'`) so no hash is needed. Inline scripts a theme outputs are emitted with a template function (§5), and the hash is registered at that time
3. **The body (`content_html`)**: not hashed. Scripts are removed by sanitization. Even if one somehow remained, the CSP stops it

---

## 4. The Plugin ABI (Breaking Change)

Plugins stop writing `<script>` in raw HTML strings and return **structured data** instead. The host validates it, builds the tag, and computes the hash. Plugins know neither hashes nor nonces.

```rust
// aegis-core
pub struct InjectedScript {
    pub source: ScriptSource,
    /// Consent category (such as `analytics`). If present, the host outputs it as `type="text/plain" data-aegis-consent="..."`
    pub consent: Option<String>,
}

pub enum ScriptSource {
    /// `<script src="https://..." async defer>`
    External { url: String, async_load: bool, defer: bool, crossorigin: bool },
    /// `<script>code</script>`
    Inline { code: String },
}
```

- Add `scripts: Vec<InjectedScript>` to `PageRenderContext` (returned by `on-render-page`; the position is `<head>` only). The ABI version is raised (`abi_version` 2). For external script URLs, this site's paths also pass without declaration (`/assets/x.js`).
- **The host removes `<script>` inside `extra_head` / `append_content` / `prepend_content`** (parsed and dropped with lol-html; `on*` attributes are dropped too). `<link>`, `<meta>`, `<style>`, and the like are as before.
- **The host's validation** (on the result of `on-render-page`, before building the tag):
    - `External`: an `https://` URL whose host is in the manifest's `capabilities.csp.script_domains`. The only attributes are `src`, `async`, and `defer`, decided by the host (`onload` and the like cannot be written; the URL is escaped correctly as an attribute)
    - `Inline`: the manifest must declare **`capabilities.inline_scripts: true`** (least privilege; without the declaration it is dropped and a warning is left). A size limit (default 64 KB)
    - The category string is only `[a-z0-9_-]{1,32}`
    - Scripts that violate this are dropped. The page is not broken, and it is recorded in the `render` warnings and the audit log
- **Hashes**: the host takes the SHA-256 of the body of each inline script it output (those output as `type="text/plain"` too, because `consent.js` runs the same body after consent) as `sha256-<base64>` and adds it to the response's CSP.
- **Caching the result of `on-render-page`** (`[runtime].render_cache_mb`) is as before. The hashes are determined by the result, so caching is fine.

---

## 5. Themes and Templates

- **Built-in templates**: stop using inline scripts and `onclick`. The comment form's script becomes an external file (served like `consent.js`, as with `/_aegis/comments.js`), and the setup screen also becomes an external file. `onclick="window.scrollTo..."` becomes binding events from an external script.
- **Third-party themes**: inline `<script>` and `on*` attributes stop working under Strict (a breaking change). There are 2 ways to fix it (the second is added in 4).
    - Make the script `themes/{name}/assets/*.js` and load it with `<script src>` (`'self'`).
    - For things that must be inline, output them with the template function `{{ inline_script("code") }}`. The host builds the tag and registers the hash.

---

## 6. Switching (`[security].strict_csp`)

**The default is `"on"`** (a breaking change).

| Value            | Behavior                                                                                                                                                                                                                  |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `"on"` (default) | Enforces the Strict CSP (removes `'unsafe-inline'` and allows inline scripts from trustworthy sources by hash)                                                                                                            |
| `"report-only"`  | Keeps the previous CSP (`script-src 'self' 'unsafe-inline'`) and also outputs the Strict CSP as `Content-Security-Policy-Report-Only`. You can check what would be blocked in the browser console. For checking migration |
| `"off"`          | Returns to the previous CSP. However, §4's validation and the removal of `<script>` always apply (the ABI v2 promise)                                                                                                     |

**What stops working with `on`**: the theme's own directly written inline `<script>` and `on*` attributes (make them external files or `{{ inline_script() }}`; [Theme authoring 6a](theme_authoring_guide.md#6a-how-to-write-scripts-do-not-write-inline)), `<script>` written directly in HTML fragments (the normal mode of `core/html` and the like are removed by sanitization anyway), and pasted code of external services that need `'unsafe-inline'` (those among AdSense, analytics, and CAPTCHA that have inline startup code; the official plugins `official-ads` and `official-clarity` are already handled with structured injection).

- When an administrator has overridden the CSP, that value takes priority and `strict_csp` has no effect.
- Public pages' `style-src` becomes Strict with the same mechanism (§6.1). For Admin Studio, see §1.1.

### 6.1 Styles (`<style>` by Hash, No `style=""` Attributes)

With `strict_csp = "on"`, `'unsafe-inline'` is also removed from `style-src`, and **the hash of the body of `<style>` elements that trustworthy sources output** is added (the same mechanism as scripts; `aegis_core::csp::record_inline_style` / `record_styles_in`; taken from the body after namespace replacement). Hashes are registered only from the following sources.

- Built-in templates: what is output with `{% filter inline_style %}…{% endfilter %}` (or `{{ inline_style("CSS") }}`). **Themes can write `<style>` the same way** (a directly written `<style>` is stopped). `extra_css` (plugin CSS and the site's custom CSS) goes into this too.
- The built-in block styles (`BLOCK_STYLES`), the `<style>` in headers written by administrators (`custom_head_code`), and built-in pages (initial setup, maintenance, newsletter sign-up).
- `<style>` that plugins return in `on-render-page` (`extra_head`, `prepend_content`, `append_content`) and `render-blocks` (the host trusts it and registers the hash; a plugin's code is trusted by the administrator who installed it). **`<style>` in a user-written body (the normal mode of `core/html`) does not get a hash registered, so it is stopped.**
- The **sandbox mode** of `core/html` (`<iframe srcdoc>`) inherits the parent's CSP, so the hashes of the inline `<style>` and `<script>` inside (those the administrator wrote, and the built-in one that syncs height) are registered too. Scripts from external hosts are only those the parent's CSP allows. `style="…"` attributes inside are also stopped (write them in `<style>`).

**`style=""` attributes cannot be used** (`style-src` also applies to attributes). The built-in templates (about 160 places), the renderer (the table's `text-align`, the sandbox's `iframe`), the contact and newsletter honeypots, and the official plugins (`official-share-buttons` uses classes in `extra_css` for per-service colors, and `official-ads` uses existing CSS) were replaced with classes. The built-in templates' classes are `aegis-s<number>` (`aegis_theme::UTILITY_STYLES`; `!important` so that they take precedence like the original `style` attributes), and for the bundled themes `ts-<number>` in each theme's `assets/style.css`. Dynamic values (the comment indent `--depth`) became the classes `aegis-depth-0` to `4`. HTML emails are built with `style` attributes (the email body has nothing to do with CSP), so only the admin screen's email preview (`/api/v1/preview/frame/…`) gets `style-src-attr 'unsafe-inline'` added.

**What stops working**: `style="…"` attributes written by themes and plugins (change them to classes and `<style>` / `extra_css`), `<style>` written directly by themes (wrap it in `inline_style`, or load `assets/*.css` with `<link>`), and `<style>` that users write in the normal mode of `core/html` (use the sandbox mode, or the theme's CSS). The previous behavior is `strict_csp = "off"`, and for checking only, `"report-only"`.

---

## 7. How the Work Was Split

1. **The design document** (this page).
2. **ABI v2** (**done**): `InjectedScript` and `capabilities.inline_scripts`, the host's validation (`aegis_runtime::scripts`; removing `<script>` and `on*` attributes with lol-html and building tags), the SDKs (Rust, AssemblyScript, TinyGo), migration of the official plugins (`page-toc`, `back-to-top`, `share-buttons`, `lightbox`, `external-links`, `code-lines`, `code-enhancer`, `mermaid`, `ads`, `clarity`), the plugin dev server's preview, and updates to [plugin_abi_specification.md](plugin_abi_specification.md), [plugin_hooks.md](plugin_hooks.md), and [versioning.md](versioning.md). The CSP is not changed yet.
    - What changed in the migration: `official-mermaid`'s `script_url` is only this site's path or an `https://` URL on `cdn.jsdelivr.net` (the host drops other hosts). `<script>` in `official-ads`'s `custom_head_code` can no longer be used (`<meta>`, `<link>`, etc. can; a safe way to add scripts is undecided). `official-code-enhancer`'s Prism runs `window.Prism.manual = true` first instead of `<script data-manual>`.
3. **Externalizing the built-in templates** (**done**): the comment section's script (`/_aegis/comments.js`; the URL is `CommentSection.script_url`), the initial setup screen (`/_aegis/setup.js`), and the analytics beacon (`/_aegis/analytics.js`; the endpoint is `data-endpoint`) became external files, and the `onclick` of an article's "Back to top" became an in-page link (`<a href='#'>`; smooth scrolling is CSS `scroll-behavior`). That the built-in templates and the 5 bundled themes output no inline code is checked by an `aegis-theme` test (render all templates and look for `<script>` (without `src`) and `on*`) and an E2E (posting a comment with a CSP of only `script-src 'self'`). The policy of not writing inline code was added to how to build themes ([theme_authoring_guide.md 6a](theme_authoring_guide.md#6a-how-to-write-scripts-do-not-write-inline)). `inline_script()` (§5) is added in 4 along with the hash registration mechanism.
4. **Collecting hashes and the CSP** (**done**): when a trustworthy source (a plugin's inline `scripts`, the template's `inline_script()`) outputs, the SHA-256 of the body is recorded with `aegis_core::csp::record_inline_script`. The recording sink is created per request by `security_headers_middleware` (`tokio::task_local`; it does nothing outside the sink). The response body undergoes namespace replacement (`aegis-` → `cms-`), so **hashes are also taken from the body after replacement** (the same replacement is passed to the sink). `headers::strict_script_csp` removes `'unsafe-inline'` and `'unsafe-eval'` from `script-src` and adds the hashes (declared hosts are kept, and `strict-dynamic` is not used). `[security].strict_csp` is `off` (default) / `report-only` (adds `Content-Security-Policy-Report-Only` in addition to the current CSP) / `on`. When an administrator has overridden the CSP, that value takes priority. Admin Studio (`/admin/`) never had `unsafe-inline`. `admin/e2e/aegis.e2e.toml` is `on`, and all E2E runs under Strict.
5. **Removing `'unsafe-inline'` from public pages' `style-src`** (**done**; a breaking change; §6.1). `<style>` by hash, no `style=""` attributes (replaced with classes). The `core/html` sandbox (`srcdoc`) had its scripts blocked even under the earlier Strict (because it inherits the parent's CSP), so it was fixed by registering the hashes of the contents.
6. **Making `on` the default** (**done**; a breaking change). The default of `[security].strict_csp` was made `"on"`. The previous behavior is `"off"`, and for checking only, `"report-only"`. At the same time, Admin Studio's `style-src` was made `'self'` only (§1.1).

### 7.1 Decided and Not Decided

- Decided: the hash approach (to coexist with shared caches), not using `strict-dynamic`, not showing nonces or hashes to plugins, and allowing a breaking ABI change.
- Decided (in 4): for `inline_script()`, the template function calls `aegis_core::csp::inline_script_tag` to build the tag and records the body's hash in the request's sink. There is no sink for `report-only` reports (check in the browser console).
- Decided (in 5): make `on` the default (no transition period; shipped as a breaking change). `aegis-server theme dev` (the theme dev server) also attaches the public site's default CSP (Strict) to the preview page (**done**). What was blocked (inline `<script>`, `<style>`, `on*` attributes, and fonts and images from undeclared hosts) is shown at the bottom right of the page with the reason and how to fix it. There are also `--csp report-only` (does not enforce) and `--csp off`. Only what the theme output with `inline_script` / `inline_style` and the dev server's own script are allowed by hash.

---

## 8. Theme Fonts and External Styles (`csp` in `theme.json`)

The public page CSP's `style-src` is `'self'` and the `<style>` hashes (§6.1), and `font-src` is `'self' data: https:`. As a result, **external stylesheets (such as `fonts.googleapis.com` for Google Fonts) are blocked by the CSP** (previously, the built-in default theme and the 4 bundled themes loaded Google Fonts, were blocked, and fell back to a substitute typeface). There are 2 ways to fix it.

- **Serve fonts yourself** (the built-in default theme's approach): place woff2 on your own site and load it with `@font-face`. It depends on nothing external, so there is nothing to add to the CSP, and the visitor's IP address does not leave the site. The built-in `base.html` serves the **Latin subsets** of Plus Jakarta Sans and JetBrains Mono from `/_aegis/fonts/<file>` (embedded in the binary; the license is SIL OFL 1.1; [`crates/aegis-server/assets/fonts/README.md`](../crates/aegis-server/assets/fonts/README.md)). Other characters, such as Japanese, fall back to system fonts.
- **Declare external hosts in `theme.json`** (when you cannot serve them yourself, like Japanese web fonts): writing `csp` in the theme's `theme.json` makes the server add them to public pages' CSP while that theme is active.

```json
{
  "id": "roastery",
  "csp": {
    "style_domains": ["fonts.googleapis.com"],
    "font_domains": ["fonts.gstatic.com"]
  }
}
```

- What you can write is `style_domains` (`style-src`), `font_domains` (`font-src`), and `img_domains` (`img-src`). **Script hosts cannot be declared** (a theme's scripts are its own files or `inline_script()`).
- Host names only (no wildcards, schemes, ports, or paths), up to 10 per list. A theme with errors cannot be installed, and errors in a running theme are ignored (with a warning).
- The admin screen's "Themes" card shows "Loads from other sites: … (visitors' IP addresses are sent)". Visitors' IP addresses reach external font CDNs, so mention it in your privacy policy.
- Among the bundled themes, `emerald-luxury`, `minimal-editorial`, `roastery`, and `trailhead`, which load Google Fonts, declare it (`cyber-neon` does not load them).

Even when an administrator has overridden the CSP in the admin screen, the declared hosts are added (the same as a plugin's `capabilities.csp`).

---

## 9. Performance Impact (Measured 2026-11-09)

We compared the performance change from security hardening (Strict CSP, removing `style` attributes, ABI v2, the CORS review, etc.) between before the hardening began (right after #297) and now. Same settings (SQLite, 2,000 articles, 6 official plugins), same load (`scripts/benchmark.mjs`, 50 concurrent, 6 seconds each, the median of 4 alternating rounds), release build.

| Scenario                 | Before hardening |         Now | Difference |
| ------------------------ | ---------------: | ----------: | ---------: |
| Routing only (health)    |      8,501 req/s | 8,382 req/s |      -1.4% |
| Top page                 |            3,744 |       3,577 |      -4.5% |
| Article page             |            1,931 |       1,784 |      -7.6% |
| Authenticated API (list) |              541 |         538 |      -0.5% |
| Admin screen preview     |            1,642 |       1,348 |     -17.9% |

- The decrease on public pages is about 0.1–0.5 ms per request. It comes from the HTML growing by about 11% (classed styles, the self-hosted font's `@font-face`) and from collecting hashes, namespace replacement, and building the CSP header. The API is unchanged.
- In the first measurement (top -22%, preview -51%), about 30 KB of CSS was run through SHA-256 on every request, fragments were duplicated in lowercase to look for `<style>`, and the built-in styles (about 16 KB) went through the scan that removes `<script>` for each `on-render-page` plugin. It was fixed as follows: (1) reuse body hashes (a body → hash cache in `aegis_core::csp`), (2) skip scanning and escaping when there is no sink or no `<`, and (3) the built-in styles bypass plugins and are added before `</head>` at the end of the theme's rendering (`ThemeEngine::render`).
- The preview is a POST used by administrators and renders the theme-applied page and runs plugin processing every time, so the difference shows more than for public pages (it has nothing to do with the public site's performance).
