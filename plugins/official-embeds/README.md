# official-embeds

**English** | [日本語](README.ja.md)

An official plugin that adds blocks that embed pages of external services in the body with an `<iframe>`: videos (YouTube, Vimeo), maps (OpenStreetMap, Google Maps), CodePen, Google Calendar, and Google Forms (an example of the [`render-blocks` hook](../../docs/plugin_hooks.md#110-render-blocks) and of [embedding external pages](../../docs/plugin_hooks.md#32-embedding-external-pages-capabilitiesembed)).

Blocks that depend on external sites are gathered in this plugin. Blocks that do not (buttons, cards, galleries, and so on) are in [`official-blocks`](../official-blocks/). A site that does not use embeds adds no hosts to its CSP as long as this plugin is not enabled.

| Block                             | Input fields                                                                                                                | Display                                      |
| --------------------------------- | --------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| `official-embeds/video`           | Video URL (required; YouTube, Vimeo), title (for screen readers), shape (16:9 / 4:3 / 9:16)                                 | A video player                               |
| `official-embeds/openstreetmap`   | Latitude (required), longitude (required), zoom (1–19, default 15), place name                                              | An OpenStreetMap map with a marker           |
| `official-embeds/codepen`         | Pen URL (required), first tab, height (200–1200px, default 400), run on click, title                                        | A CodePen pen                                |
| `official-embeds/google-map`      | Embed code (required), shape (16:10 / 4:3 / 1:1), place name                                                                | A Google Maps map                            |
| `official-embeds/google-calendar` | Calendar (required; an ID or the embed code), view (month / week / agenda), time zone, height, title                        | A Google Calendar                            |
| `official-embeds/google-form`     | Form URL (required; a URL or the embed code), height (300–3000px, default 800), title                                       | A Google Forms form                          |
| `official-embeds/iframe`          | Page URL (required; a host the administrator allows), title (required), height (100–3000px, default 500), allow full screen | A page on an allowed site (with a `sandbox`) |

## Common Behavior

- **Entered URLs and HTML are never output as they are.** The plugin extracts and checks only the values it needs (the video ID, the pen ID, the calendar ID, and so on) and builds the `<iframe>`'s `src` itself. When you paste a service's embed code (`<iframe ...>`), only the values inside its `src` are used (attributes such as `style` are dropped).
- Blocks with an unreadable URL or a malformed value are not shown.
- Only the hosts written in the manifest's `capabilities.embed.frame_domains` can be embedded. While the plugin is enabled, these hosts are added automatically to the CSP `frame-src`.

| Service         | Host added to `frame-src`  |
| --------------- | -------------------------- |
| YouTube         | `www.youtube-nocookie.com` |
| Vimeo           | `player.vimeo.com`         |
| OpenStreetMap   | `www.openstreetmap.org`    |
| CodePen         | `codepen.io`               |
| Google Maps     | `www.google.com`           |
| Google Calendar | `calendar.google.com`      |
| Google Forms    | `docs.google.com`          |

- The CSP works per host. For example, adding `www.google.com` also lets other google.com pages be framed (only the `/maps/embed` URL this plugin builds goes into the body).
- **Privacy**: showing an embed sends the viewer's IP address and other details to that service, and some services use cookies. State the external services you use in your privacy policy. When you write a consent category in `[embed].consent`, embeds are not loaded until that category is consented to ([Cookie consent 2.3](../../docs/consent.md#23-embeds-in-plugin-blocks); this plugin needs no settings).
- It does not use server-side outbound network requests or the KVS (the viewer's browser loads the embeds).

## Video (YouTube, Vimeo)

- For YouTube, it reads `youtube.com/watch?v=`, `youtu.be/`, `youtube.com/shorts/`, `youtube.com/embed/`, and `youtube.com/live/` URLs, and also uses the start time (`t=90`, `t=1m30s`, `start=90`). For Vimeo: `vimeo.com/<ID>` (unlisted `vimeo.com/<ID>/<hash>`) and `player.vimeo.com/video/<ID>`.
- YouTube is embedded in privacy-enhanced mode (`www.youtube-nocookie.com`), and Vimeo with `dnt=1` (no tracking of views).

## Map (OpenStreetMap)

- No API key is needed. It does not search by address; you specify latitude and longitude (right-click a place on openstreetmap.org to copy its coordinates).
- If the latitude is outside -85 to 85 or the longitude is outside -180 to 180, the block is not shown. Below the map it shows a "View larger map" link and the copyright notice (© OpenStreetMap).

## CodePen

- Paste a `codepen.io/<user>/pen/<ID>` URL (`/full/`, `/details/`, and `/embed/` also work) or the embed code from CodePen's "Embed".
- "First tab" chooses whether to show only the result, or one of HTML, CSS, and JS next to the result.
- With "Run on click", a preview is shown and the pen runs only when the visitor clicks it (for heavy pens, or several on one page).
- Private pens can only be embedded with a paid CodePen plan.

## Google Maps

- Open the place in Google Maps and paste the code from Share › Embed a map › Copy HTML (only the `https://www.google.com/maps/embed?pb=...` URL also works). No API key is needed.
- Only the URL's `pb` value is used. URLs other than `www.google.com/maps/embed` (the map page's URL, short links, and so on) are not read.
- The place name is shown under the map and read aloud for the frame.

## Google Calendar

- The calendar must be **public** (Google Calendar settings › Access permissions › Make available to public). Otherwise viewers cannot see its events.
- In the input field, paste the calendar ID (`xxxx@group.calendar.google.com`, `xxxx@gmail.com`) or the embed code / URL from Integrate calendar in the settings. Several calendars in the embed code are kept as they are (up to 10).
- With an empty time zone (such as `Asia/Tokyo`), the one in the embed code is used, or else the calendar's own.
- Fine settings in the embed code, such as colors and shown items, are not carried over (only the calendars, the time zone, and the view).

## Google Forms

- Paste the code from the form's Send › `<>` (embed HTML), or the form's URL (`docs.google.com/forms/d/e/<ID>/viewform`).
- Short links (`forms.gle/...`) cannot be used, because the ID is unknown without opening them. Open one in a browser and copy the full URL.
- The form's height does not change automatically. Adjust "Height" to the number of questions (if it is too small, a scroll bar appears inside the form).

## Embed (Allowed Sites)

- A generic block that embeds pages on hosts the site's administrator allows. Allow hosts in Admin Studio's **Settings › Headers** ("Hosts Allowed for Embeds") or in `[embed].frame_domains` in the config file ([Configuration 2.23a](../../docs/configuration.md#223a-embed--embeds-iframe-in-plugin-blocks)). The manifest's `capabilities.embed.site_domains` declares that this plugin accepts those hosts.
- This plugin does not know the list of allowed hosts. It only checks that the URL is `https://` and that the host name is well formed; iframes from hosts that are not allowed are removed by the server's sanitizing (they do not appear in the editor preview either).
- It adds `sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox"`. The embedded page works, but it cannot move this site's page (the parent) to another URL.
- The allowed hosts are added to the CSP `frame-src` while this plugin is enabled.

## Usage

Add `"official-embeds"` to `[plugins].enabled` in the config file, or enable it in Admin Studio's **Settings › Plugins**. "Video", "Google Maps", and the other blocks then appear in the editor's add-block button.

```toml
[plugins]
enabled = ["official-embeds"]
```

- Layout and spacing use zero-specificity CSS (`:where()`). To change them in a theme, target classes such as `.oe-frame`, `.oe-video`, and `.oe-map`.

## Build

```bash
task plugin:build:embeds   # rebuild plugin.wasm (requires rustup target add wasm32-unknown-unknown)
cargo test -p aegis-plugin-embeds
```
