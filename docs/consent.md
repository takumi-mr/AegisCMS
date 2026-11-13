[← Back to the documentation index](README.md)

**English** | [日本語](consent.ja.md)

# Cookie Consent Banner

Shows a banner that asks for consent, for sites that use cookies or scripts requiring visitor consent, such as ads, analytics, and external videos or maps (`[consent]`). It keeps the visitor's choice in a first-party cookie and **does not run scripts or iframes that need consent until consent is given**.

AegisCMS itself uses only the cookies needed to run the site (such as the admin session). Analytics (`[analytics]`) uses no cookies either. This banner is for managing consent for the ads, analytics, and embeds **that you add**.

The banner's **HTML and wording are decided by the theme's template `consent.html`** (the built-in one if the theme has none). **How it is displayed** (shape, position, backdrop, delay) can be changed in the settings.

```text
 Inserted into public pages by the server (regardless of the theme; also on the 404 page)
   <head> ─ <link>   /_aegis/consent.css       Styles for the built-in consent.html
          ─ <script type="application/json" id="aegis-consent-config">  Behavior settings (data, not executed)
          ─ <script src="/_aegis/consent.js" defer>
   Before </body> ─ the banner's HTML      The theme's templates/consent.html (or the built-in one) rendered
        │
        ├─ No choice yet / the categories changed ──▶ show the banner (Reject all / Choose / Accept all)
        └─ A choice exists ──▶ run the scripts and iframes of the allowed categories (data-aegis-consent)
```

---

## 1. Enabling It

```toml
[consent]
enabled = true
policy_url = "/privacy"        # link to the privacy policy (optional)
```

After restarting the server, the banner appears on all public pages. The wording is in the `[theme].locale` language (`en` / `ja`), and if you do not decide the categories, they are `analytics` (analytics) and `marketing` (marketing).

```toml
[consent]
enabled = true
layout = "box"                 # banner (a strip at the screen edge) / box (a card in a corner) / modal (centered; dims the background)
position = "bottom-left"       # position per shape. If omitted, the shape's default
backdrop = false               # dim the background and make the page non-interactive. If omitted, true for modal only
delay_ms = 1500                # delay the display (milliseconds)
cookie_days = 180              # days to keep the choice (1–395)
policy_url = "/privacy"
message = ""                   # when changing the description (if empty, the theme's default text)
reopen_button = true           # the "Cookie settings" button after a choice is made
revision = 1                   # increase it when you want to ask everyone again

[[consent.categories]]
id = "analytics"               # uses the built-in name and description
[[consent.categories]]
id = "marketing"
[[consent.categories]]
id = "video"                   # categories without a built-in name need a label
label = "Embedded videos"
description = "Shows videos such as YouTube."
```

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | `true` shows the banner |
| `layout` | string | `"banner"` | The banner's shape. `banner` / `box` / `modal` |
| `position` | string | The shape's default | Position. `banner`: `bottom` (default) / `top`; `box`: `bottom-right` (default) / `bottom-left` / `top-right` / `top-left`; `modal`: `center` |
| `backdrop` | bool | `true` for `modal` only | Dim the background and make the page non-interactive |
| `delay_ms` | integer | `0` | Time to delay the display (0–10000) |
| `cookie_days` | integer | `180` | Days to keep the choice (1–395) |
| `policy_url` | string | `""` | URL of the privacy policy (a path starting with `/` or `http(s)://`). If empty, no link is shown |
| `message` | string | `""` | The description (up to 600 characters). If empty, the theme's default text |
| `reopen_button` | bool | `true` | A small button shown after a choice, to reopen the settings |
| `revision` | integer | `1` | Increasing it invalidates saved choices and asks again |
| `categories` | table[] | `analytics` and `marketing` | The categories to ask consent for (up to 10) |

- A category `id` starts with a lowercase letter, then lowercase letters, digits, `-`, and `_` (up to 32 characters). `necessary` (required) is always on and cannot be defined.
- `analytics`, `marketing`, `preferences`, and `functional` get a name and description in the theme's language. Other `id`s need a `label` (up to 60 characters). `description` is up to 300 characters.
- **If the sequence of category `id`s or the `revision` changes, everyone is asked again** (changing only a name, description, or shape does not ask again).
- If a value is invalid, the server does not start (for example, a position that does not fit the shape).

## 2. Writing Scripts That Need Consent

In theme templates, and in the HTML a plugin outputs to `extra_head` / the body, write anything that needs consent in a form that **does not run**. When consent is given, scripts run and iframes get their `src`.

```html
<!-- make type text/plain and write the category in data-aegis-consent -->
<script type="text/plain" data-aegis-consent="analytics"
        src="https://analytics.example.com/tag.js" async></script>

<script type="text/plain" data-aegis-consent="marketing">
  window.adQueue = window.adQueue || [];
</script>

<!-- write iframes in data-src, not src -->
<iframe data-aegis-consent="video" data-src="https://www.youtube-nocookie.com/embed/ID"
        width="560" height="315" title="Video"></iframe>
```

- **Nothing runs or loads while consent is absent.** Even if the banner's script cannot be loaded (blocked), or the theme's `consent.html` is broken, things that need consent do not run.
- Attributes (`src`, `async`, `data-*`, etc.) are carried over when running. Inline scripts are run as is.
- **When permission is withdrawn** (when you reopen the settings and turn off a category that was allowed), scripts that already ran cannot be stopped, so the page is reloaded.
- Official plugin examples: [`official-ads`](../plugins/official-ads/) (ads; category `marketing`) and [`official-clarity`](../plugins/official-clarity/) (heatmaps and session recording; category `analytics`) output in this form.
- From a plugin, just put the category in the **`consent` of `scripts` (structured injection)** of `on-render-page`. The host builds the tag in the form above (the plugin does not need to assemble HTML, and the host strips `<script>` written in `extra_head` and the like; [ABI specification 5.3.3a](plugin_abi_specification.md#533a-adding-scripts-in-on_render_page-scripts-abi-2)). The plugin does not need to know about the consent mechanism.

### 2.1 From JavaScript

```js
window.AegisConsent.has('marketing');        // whether it is allowed
window.AegisConsent.get();                   // { analytics: true, marketing: false }. null if not yet chosen
window.AegisConsent.onChange((choices) => {  // called when the choice becomes known or changes (once right now if already known)
  if (choices.analytics) startTracking();
});
document.addEventListener('aegis:consent', (e) => console.log(e.detail.categories));
window.AegisConsent.open();                  // open the settings
```

### 2.2 A Link to Reopen the Settings

Place a link or button with `data-aegis-consent-open` in the footer or elsewhere, and clicking it opens the settings.

```html
<a href="#" data-aegis-consent-open>Cookie settings</a>
```

## 3. Building the Banner in a Theme (`templates/consent.html`)

Placing `templates/consent.html` in a theme lets you decide the banner's HTML, wording, and look entirely. The built-in template is a **fragment** that does not extend `base.html`, and the server inserts it before `</body>` of each page (do not `extends` `base.html`).

### 3.1 Values Passed to the Template

| Value | Description |
| --- | --- |
| `fields.consent.layout` | `banner` / `box` / `modal` |
| `fields.consent.position` | Position (`bottom`, `top`, `bottom-right`, `bottom-left`, `top-right`, `top-left`, `center`) |
| `fields.consent.backdrop` | Whether to dim the background (`true` / `false`) |
| `fields.consent.policy_url` | URL of the privacy policy (empty if none) |
| `fields.consent.reopen_button` | Whether to show the button to reopen the settings |
| `fields.consent.delay_ms` / `cookie_days` | The display delay and the days to keep the choice |
| `fields.message` | `[consent].message` (if empty, use your own text such as the theme's `t("consent.message")`) |
| `fields.categories` | The list of categories. `id`, `label`, `description` (for built-in categories, empty if no name is decided) |
| `fields.preview` | `true` in the theme dev server's preview (shown from the start) |
| `site_title`, `locale`, `base_path`, `t("...")` | Same as other templates (built-in strings such as `t("consent.title")` can be used too) |

### 3.2 Attributes That Tie Into the Script

The script finds and operates elements by the following attributes. **The look and structure are free**, but keep these attributes.

| Attribute | Role |
| --- | --- |
| `data-aegis-consent-root` | The whole dialog. **`hidden` at first** (the script shows it). While open, `data-aegis-consent-state="ask"` (the first question) or `"details"` (choosing categories) |
| `data-aegis-consent-action="reject-all"` / `"accept-all"` | Reject all / accept all and close. **The initial focus goes to `reject-all`** |
| `data-aegis-consent-action="customize"` | Opens the category selection (it hides itself when opened) |
| `data-aegis-consent-details` | The part that holds the category checkboxes. **`hidden` at first** |
| `input[data-aegis-consent-category="<category ID>"]` | The checkbox per category (the script fills it with the current choice) |
| `data-aegis-consent-action="save"` | Saves with the chosen categories and closes. **`hidden` at first** |
| `data-aegis-consent-reopen` | The button shown after a choice to reopen the settings. **`hidden` at first** |

- You can also build a banner with only two buttons, without "Choose" (`customize` / `details` / `save`). In that case `accept-all` allows all categories and `reject-all` rejects all categories.
- When `layout` is `modal`, the script traps Tab key focus inside the dialog (add `role="dialog"`, `aria-modal="true"`, and `aria-labelledby` / `aria-describedby` pointing to the heading and description).
- Escape closes it only when a choice has been made (the first question does not close until answered).
- In a template without `data-aegis-consent-root`, the banner does not appear (things that need consent remain not running; a warning appears in the browser console).

### 3.3 Example: A Banner with Only Two Buttons

```html
<aside data-aegis-consent-root hidden class="cookie-notice cookie-notice--{{ fields.consent.layout }}"
       role="dialog" aria-labelledby="cn-title">
  <p id="cn-title">{{ fields.message or "This site uses cookies." }}</p>
  <button type="button" data-aegis-consent-action="reject-all">Don't use</button>
  <button type="button" data-aegis-consent-action="accept-all">Allow</button>
</aside>
```

### 3.4 Appearance

The built-in template's styles are in `/_aegis/consent.css`, all with zero-specificity `:where()`. You can override them by specifying `.aegis-consent-*` in the theme's CSS. When building your own template, you may write in the theme's CSS without using this file's classes.

| Class | Part |
| --- | --- |
| `.aegis-consent` / `.aegis-consent-layout-<shape>` / `.aegis-consent-pos-<position>` / `.aegis-consent-backdrop` | The outer frame (shape, position, backdrop) |
| `.aegis-consent-body` | The main card |
| `.aegis-consent-title` / `.aegis-consent-message` / `.aegis-consent-policy` | The heading, description, and link to the policy |
| `.aegis-consent-details` / `.aegis-consent-category` | The list of categories and a single category |
| `.aegis-consent-actions` / `.aegis-consent-button` | The row of buttons and a button (reject and accept look the same) |
| `.aegis-consent-reopen` | The "Cookie settings" button after a choice |

### 3.5 Checking While You Build

`aegis-server theme dev` (and Theme Builder) displays `templates/consent.html` as the `consent` sample. The banner is shown from the start (`fields.preview`) with the built-in styles. The sample can be changed in `samples/consent.json` (write `{"fields": {...}}`; `fields` is replaced as a whole, so write all the values you use). The actual behavior (what the buttons do) is attached by the script when it is inserted into the site, so the preview only checks the look.

## 4. Mechanism and Notes

- **CSP-friendly**: the script and CSS are external files (`/_aegis/consent.js` / `consent.css`; the `/_<namespace>/` name changes with `[server].namespace`), and the settings are non-executed JSON (`type="application/json"`), so it works even with a CSP of only `script-src 'self'`. When there is a `[server].base_path`, it is added to the URLs.
- **Insertion**: added before `</head>` and before the last `</body>` of `text/html` `GET` responses. It does not touch the API (`/api/`), Admin Studio, uploads, or `/_aegis/`. With `[consent].enabled = false` it does nothing (it does not even read the body). If `consent.html` cannot be rendered (a template error), only the banner is not shown and the page is returned as is (an error is logged).
- **Storing the choice**: the cookie `aegis_consent` (`Path=/`, `SameSite=Lax`, `Secure` on HTTPS, `cookie_days` days). The value is `{"r": revision, "c": {category: true/false}}`. It is only sent to the server and not stored or recorded (if recording consent is legally required, arrange that separately).
- **Global Privacy Control**: the initial state is always "all off", and it never allows anything on its own.
- **When you change the settings**: restart the server. When pages are cached with `[cdn]`, the inserted settings (JSON) and banner HTML are cached too, so purge the CDN cache after restarting.
- The settings are in the config file only (there is no screen in Admin Studio).
- Check the legal requirements (target countries, records, disclosing the list of cookies, etc.) per site. This banner is a component for satisfying "do not run before consent", "rejecting is as easy as accepting", and "can be changed at any time".
