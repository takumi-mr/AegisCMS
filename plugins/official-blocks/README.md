# official-blocks

**English** | [日本語](README.ja.md)

An official plugin that adds blocks you can place in the body: button, link card, video, audio, file, map, image gallery, FAQ, steps, columns, image and text, image with caption, timeline, chat bubbles, tabs, stats, definition list, and rating (review). It is also a model example for building custom blocks with plugins (the [`render-blocks` hook](../../docs/plugin_hooks.md#110-render-blocks); for video, [embedding external pages](../../docs/plugin_hooks.md#32-embedding-external-pages-capabilitiesembed). Both links are Japanese documents).

| Block | Input fields | Display |
| --- | --- | --- |
| `official-blocks/button` | Text (required), link target (required), look (filled / outlined), position (left / center) | A link shaped like a button |
| `official-blocks/link-card` | Link target (required), title (required), description, image | A card showing the title, description, image, and the host name for external links |
| `official-blocks/video` | Video URL (required; YouTube, Vimeo), title (for screen readers), shape (16:9 / 4:3 / 9:16) | A video player |
| `official-blocks/audio` | Audio URL (required), title | The browser's standard audio player (not loaded until played) |
| `official-blocks/file` | File URL (required), title (required), description, size (shown as written) | A card for downloading |
| `official-blocks/map` | Latitude (required), longitude (required), zoom (1–19, default 15), place name | An OpenStreetMap map with a marker |
| `official-blocks/gallery` | Images and descriptions (required; items can be added), number of columns (2–4, default 3) | An image grid (2 columns on narrow screens) |
| `official-blocks/faq` | Questions and answers (required; items can be added) | A collapsible FAQ (with schema.org FAQPage microdata) |
| `official-blocks/steps` | Headings and descriptions (required; items can be added) | Steps connected by numbers and a line |
| `official-blocks/columns` | Image, heading, text (required; items can be added; up to 4 columns) | Equal-width columns (stacked vertically on narrow screens) |
| `official-blocks/media-text` | Image (required), image description, heading, text, image position (left / right) | An image and text side by side (stacked vertically on narrow screens) |
| `official-blocks/figure` | Image (required), image description (alt), caption | An image with a caption below it (`<figcaption>`) |
| `official-blocks/timeline` | Date, heading, description (required; items can be added) | A timeline connected by a vertical line (history, roadmap) |
| `official-blocks/chat` | Name, icon, position (left / right), lines of dialogue (required; items can be added) | A conversation in speech bubbles with icons and names |
| `official-blocks/tabs` | Tab names and contents (required; items can be added; up to 8), format (text / code) | Switchable tabs (no JavaScript) |
| `official-blocks/stats` | Number, unit, label (required; items can be added; up to 6) | Cards showing big numbers |
| `official-blocks/definition-list` | Terms and descriptions (required; items can be added) | A `<dl>` list (stacked vertically on narrow screens) |
| `official-blocks/rating` | Name of the subject (required), type of subject, rating 1–5 (required), summary, reviewer, per-item scores | A star rating box (with schema.org Review microdata) |

## Video

- For YouTube, it reads `youtube.com/watch?v=`, `youtu.be/`, `youtube.com/shorts/`, `youtube.com/embed/`, and `youtube.com/live/` URLs, and also uses the start time (`t=90`, `t=1m30s`, `start=90`). For Vimeo: `vimeo.com/<ID>` (unlisted `vimeo.com/<ID>/<hash>`) and `player.vimeo.com/video/<ID>`.
- It extracts only the video ID from the entered URL, and the plugin builds the player URL itself. YouTube uses privacy-enhanced mode (`www.youtube-nocookie.com`), and Vimeo uses `dnt=1` (no tracking of views).
- Only the manifest's `capabilities.embed.frame_domains` (`www.youtube-nocookie.com`, `player.vimeo.com`) can be embedded, and these hosts are added automatically to the CSP `frame-src` while the plugin is enabled.
- Blocks with an unreadable URL are not shown.
- Watching a video sends the viewer's IP address and other details to YouTube / Vimeo. State in your privacy policy that you use external video services.

## Map

- It is an OpenStreetMap embed (`www.openstreetmap.org`). No API key is needed. It does not search by address; you specify latitude and longitude (right-click a place on openstreetmap.org to copy its coordinates).
- If the latitude is outside -85 to 85 or the longitude is outside -180 to 180, the block is not shown. Below the map it shows a "View larger map" link and the copyright notice (© OpenStreetMap).
- `www.openstreetmap.org` is declared in `capabilities.embed.frame_domains` and is added to the CSP `frame-src` while the plugin is enabled. Viewing a map sends the viewer's IP address and other details to OpenStreetMap.

## Blocks with Multiple Items (Gallery, FAQ, Steps, Columns)

You enter them in input fields whose items can be added (`list`; items can be added, removed, and reordered).

- **Gallery**: up to 24 pairs of image and description. The description becomes the `alt` and the text below the image. Images other than `http(s)` or relative URLs are skipped.
- **FAQ**: up to 50 pairs of question and answer. Each line of an answer is a paragraph. Items with an empty question or answer are not shown.
- **Steps**: up to 30 pairs of heading and description. Items with an empty heading are not shown.
- **Columns**: up to 4 columns of image, heading, and text (equal width; stacked vertically on narrow screens). Columns with empty content are not shown.
- Text is plain text, and HTML is escaped.
- Bodies saved in the old format (multi-line strings: for the gallery, `URL | description` per line; for FAQ and steps, items separated by blank lines) are displayed as they are.
- The FAQ structured data is added as microdata because JSON-LD in a `<script>` is removed by sanitization. How it appears in search results is up to the search engine.
- Image links (such as enlarging with the lightbox plugin) work the same as for images in the body.

## Tabs, Chat Bubbles, and More

- **Tabs**: switching uses only radio buttons and CSS (no JavaScript or CSP change is needed). Switching with the arrow keys is the radio buttons' standard behavior. When printing, all contents are printed in order, including hidden tabs. In-page search does not find text in hidden tabs. So that multiple tab blocks on the same page do not get their switching mixed up, the radio buttons' `name` and `id` include the block position and a hash of the contents (when separate bodies are lined up on one page, like an article list, tab blocks with the same position and the same contents across bodies may switch in sync).
- **Chat bubbles**: the icon is optional, and its `alt` is empty because it is decorative. The name is displayed, so screen readers also tell who is speaking.
- **Timeline, stats, definition list, chat bubbles, tabs**: text is plain text, and HTML is escaped. Each line is a paragraph (a timeline description is put on one line). Items with an empty required input field are not shown.

## Rating (Review)

- Ratings are rounded to 0.5 steps (anything outside 1–5 is not shown). Per-item scores are 0–5, up to 10 of them. The stars are drawn with CSS, and "4.5 / 5" is used for screen readers.
- Structured data uses schema.org `Review` (`itemReviewed`, `reviewRating`, `reviewBody`, `author`) as **microdata** (JSON-LD in a `<script>` is removed by sanitization).
- Whether rich results (star display) appear in search results is up to the search engine. Google requires the reviewer (`author`) and excludes self-serving reviews of businesses or products the site itself operates. Choose a "type of subject" only when it fits one of product, software, book, or movie (otherwise `Thing`).

## Usage

Add `"official-blocks"` to `[plugins].enabled` in the config file, or enable it in Admin Studio's **Settings › Plugins**. "Button" and "Link card" then appear in the editor's add-block button.

```toml
[plugins]
enabled = ["official-blocks"]
```

- Blocks with an empty required input field, and blocks whose link target is other than `http(s)`, `mailto`, or a relative URL, are not shown.
- Layout and spacing use zero-specificity CSS (`:where()`); only the link color and underline use a specificity of two classes (such as `.ob-button-row .ob-button`) so they do not lose to the theme's body link styles. Colors use the theme's `--accent` and, for the text of filled buttons, `--accent-contrast` (white if absent; specify a dark color on themes with a light `--accent`). To change them in a theme, specify with a selector of higher specificity.
- It does not use server-side outbound network requests or the KVS (for video, the viewer's browser loads the player).

## Build

```bash
task plugin:build:blocks   # rebuild plugin.wasm (requires rustup target add wasm32-unknown-unknown)
cargo test -p aegis-plugin-blocks
```
