# Fonts of the built-in default theme

The built-in `base.html` (the default theme without a `themes/default/templates/`) serves these two fonts itself, at
`/_aegis/fonts/<file>` (`handlers::site_assets::font_file`), so it never asks a font CDN for them
(the CSP does not need a font host, and the visitor's IP address does not leave the site).

| File | Font | Version | License |
|---|---|---|---|
| `plus-jakarta-sans-latin.woff2` | [Plus Jakarta Sans](https://github.com/tokotype/PlusJakartaSans) (variable, weight 200–800) | Google Fonts v12 | SIL Open Font License 1.1 — [`OFL-PlusJakartaSans.txt`](OFL-PlusJakartaSans.txt) |
| `jetbrains-mono-latin.woff2` | [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono) (variable, weight 100–800) | Google Fonts v24 | SIL Open Font License 1.1 — [`OFL-JetBrainsMono.txt`](OFL-JetBrainsMono.txt) |

Both are the **Latin subset** (`unicode-range: U+0000-00FF, …`) as served by Google Fonts for `latin`; other scripts use
the system font (Japanese text falls back to the system Japanese font). The fonts are used unmodified, as web fonts,
which the OFL allows; the license texts travel with them.

To update: download the `latin` `src: url(...)` of the family from
`https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@400..800&family=JetBrains+Mono:wght@400..500&display=swap`
(request it with a current browser `User-Agent` to get `woff2`), replace the file, and keep the license text.
