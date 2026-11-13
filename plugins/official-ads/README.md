# official-ads

**English** | [日本語](README.ja.md)

The official AegisCMS ad delivery plugin.
It supports delivering Google AdSense and custom banner / affiliate ads, and provides script tag injection into `<head>`, ad placement in the body through a custom block (`official-ads/display`), automatic insertion at the start and end of the body, dynamic CSP (Content Security Policy) integration, and integration with the cookie consent banner (`[consent]`).

---

## Key Features

1. **Google AdSense integration**:
    - Just set the publisher ID (`client_id`), and the `on-render-page` hook safely injects the ad script into `<head>` automatically.
    - Supports formats such as responsive, rectangle, horizontal, and vertical.
2. **Custom block `official-ads/display`**:
    - Lets you freely place ad units in the body of articles and static pages from the editor.
    - Supports per-slot-ID settings, and displaying arbitrary affiliate tags and custom banner HTML.
3. **Automatic insertion into the body**:
    - Without placing blocks by hand, the setting (`auto_insert_position`) can automatically insert an ad unit at the start of the article body, the end, or both.
    - To hide ads on a specific article, set `hide_ads: true` in the article's custom field to exclude it.
4. **Integration with the dynamic CSP settings**:
    - When the plugin is enabled, the external domains needed for Google AdSense and ad delivery (`pagead2.googlesyndication.com`, `googleads.g.doubleclick.net`, `tpc.googlesyndication.com`, `ep2.adtrafficquality.google`) are automatically merged dynamically into the server's CSP (`script-src`, `frame-src`, `connect-src`).
5. **Full support for the cookie consent banner (`[consent]`)**:
    - When `require_consent = true` (the default), loading and running the ad script is held automatically until the visitor agrees to the "marketing" category on the cookie consent banner (`data-aegis-consent="marketing"`).
    - As soon as the consent button is pressed, ads are loaded and start being delivered.

---

## Configuration

Configure it in Admin Studio (Admin > Plugins > the ad delivery plugin) or in the config file `aegis.toml`.

```toml
[plugins]
enabled = ["official-ads"]

[plugins.settings.official-ads]
client_id = "ca-pub-1234567890123456"       # Google AdSense publisher ID
default_slot = "1234567890"                  # Default ad slot ID
auto_insert_position = "bottom"              # Automatic insertion into the body: none / top / bottom / both
require_consent = true                       # Integrate with the cookie consent banner (default: true)
consent_category = "marketing"               # Consent category (default: marketing)
content_types = "article"                    # Content types to deliver ads on (comma-separated)
```

---

## Custom Block Usage

In the editor's add-block menu, choose "Ad / Banner" (`official-ads/display`).

- **AdSense unit**:
    - `slot`: ad slot ID (if not specified, the `default_slot` setting is used)
    - `format`: `auto` (automatic / responsive), `rectangle`, `horizontal`, `vertical`
    - `align`: `center`, `left`, `right`
- **Custom banner / affiliate**:
    - `custom_html`: `<a href="https://example.com"><img src="https://example.com/banner.png" alt="Ad"></a>`
    - When `custom_html` is specified, it is shown in preference to AdSense. When the cookie consent banner integration is enabled, consent attributes are added automatically to iframes and scripts.

---

## Tags Usable in `custom_head_code`

HTML written in `custom_head_code` (the admin screen's "Custom header tags") goes into `<head>` as is, but **the host strips `<script>` and `on*` attributes (such as `onload`)**. Scripts can only be added by the plugin itself, and only from hosts the plugin has declared (the AdSense script is added from `client_id`). `<meta>`, `<link>`, and `<style>` can be used. For the background, see the [CSP policy](../../docs/csp.md).
