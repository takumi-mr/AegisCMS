# Changelog

## [0.3.0](https://github.com/takumi-mr/AegisCMS/compare/sdk-assemblyscript-v0.2.0...sdk-assemblyscript-v0.3.0) (2026-11-10)


### ⚠ BREAKING CHANGES

* **plugin:** PLUGIN_ABI_VERSION is 2. A <script> written into extra_head, prepend_content or append_content is removed; use PageRenderContext.scripts. official-mermaid script_url is limited to a path on the site or cdn.jsdelivr.net, and a <script> in official-ads custom_head_code no longer works.

### Features

* **plugin:** add structured script injection (ABI 2) and drop raw scripts from plugin HTML ([#301](https://github.com/takumi-mr/AegisCMS/issues/301)) ([0a19398](https://github.com/takumi-mr/AegisCMS/commit/0a19398e52b62eed21d0a141a7c10af41058b7ae))
* **plugin:** align AssemblyScript and TinyGo SDKs, templates and examples with ABI 2 ([#322](https://github.com/takumi-mr/AegisCMS/issues/322)) ([5c22976](https://github.com/takumi-mr/AegisCMS/commit/5c229768c852f8ec03e5fb4e327ad35a9d20d14f))

## [0.2.0](https://github.com/takumi-mr/AegisCMS/compare/sdk-assemblyscript-v0.1.1...sdk-assemblyscript-v0.2.0) (2026-11-05)


### ⚠ BREAKING CHANGES

* **server:** plugin HTTP requests to ports other than 80/443/8080 or to internal IP addresses are now refused, and redirects are followed (with the allowed domains checked on every hop) instead of being returned as 3xx.

### Features

* **server:** oEmbed provider, stricter plugin network policy and official-link-embed ([#226](https://github.com/takumi-mr/AegisCMS/issues/226)) ([a907365](https://github.com/takumi-mr/AegisCMS/commit/a9073653879b7f484d7036c6c6055521ce8addd4))

## [0.1.1](https://github.com/takumi-mr/AegisCMS/compare/sdk-assemblyscript-v0.1.0...sdk-assemblyscript-v0.1.1) (2026-11-04)


### Performance

* **runtime:** add append slots and input projection to on-render-page ([#209](https://github.com/takumi-mr/AegisCMS/issues/209)) ([8cf01b0](https://github.com/takumi-mr/AegisCMS/commit/8cf01b074fb95d885dabd4bd3602a64a7517ea7d))
