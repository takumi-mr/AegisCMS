# official-clarity

**English** | [日本語](README.ja.md)

An official Wasm plugin that adds [Microsoft Clarity](https://clarity.microsoft.com/) (heatmaps and session recordings) to public pages. It works just by entering the Clarity project ID.

## What It Does

- **`on-render-page`** puts Clarity's standard startup code into `<head>`.
- **Waits for cookie consent**: By default it is emitted as `type="text/plain"` with `data-aegis-consent="analytics"`, and the consent banner's (`[consent]`) script runs it after the visitor consents ([consent.md](../../docs/consent.md)). **Clarity does not run unless `[consent].enabled = true`**. Set `require_consent = false` only on sites that need no consent.
- **Adds the CSP automatically**: When the plugin is enabled, the following hosts from Clarity's official guidance ([Clarity Content Security Policy](https://learn.microsoft.com/en-us/clarity/clarity-csp)) are added to the CSP. You do not need to rewrite the site-wide CSP.
    - `script-src`: `www.clarity.ms`, `scripts.clarity.ms`
    - `connect-src`: `www.clarity.ms`, `scripts.clarity.ms`, `a.clarity.ms` to `z.clarity.ms`, `c.bing.com`
    - `img-src`: `c.clarity.ms`, `c.bing.com`
    - A wildcard (`*.clarity.ms`) cannot be written for CSP hosts, so the individual hosts from the official guidance are listed. `scripts.clarity.ms` is not in the official guidance and is based on user reports ([microsoft/clarity#913](https://github.com/microsoft/clarity/issues/913)). If Clarity's hosts change, fix `capabilities.csp` in `plugin.manifest.json`.

## Pages That Are Not Recorded

The startup code is not emitted on pages that must not be included in recordings.

- **Member, shopping, contact, and forum pages** (templates `account.html`, `cart.html`, `order.html`, `contact.html`, `forum_index.html`, `forum_category.html`, `forum_thread.html`). This is because some forum categories can be read only by members.
- **Articles and pages that require a membership level** (`access` is other than `public`). This is because the full text would be included in the recording. You can turn this off with `exclude_restricted = false`, but use it only after understanding that paid body text will remain on Microsoft's servers.
- **The admin screen preview** (an `srcdoc` iframe) and pages opened with `file:`. At the start of the startup code, it does nothing when the page is not `http(s)` (so that the preview being edited does not mix into the site's data).

Clarity itself also masks the contents of input fields by default. However, the text of the body is included in recordings.

## Settings (`[plugins.settings.official-clarity]`)

```toml
[plugins]
enabled = ["official-clarity"]

[plugins.settings.official-clarity]
project_id = "abc123xyz9"     # The project ID in Clarity's Settings > Overview (alphanumeric). Nothing is emitted if empty
require_consent = true        # Wait for consent (default true. Requires [consent].enabled = true)
consent_category = "analytics" # The consent category to wait for (default analytics)
exclude_restricted = true     # Do not add it to pages that require a membership level (default true)
```

It can also be configured from the admin screen's "Plugins".

## Privacy

Clarity is a Microsoft service, and screen recordings, click positions, and browser information are sent to Microsoft. It uses cookies. **State the transmission to the external service and its purpose in your privacy policy.** The required notices and consent requirements differ depending on the regions your site targets.

## Build

A built `plugin.wasm` is bundled. After changing the source, rebuild with `task plugin:build:clarity`.
