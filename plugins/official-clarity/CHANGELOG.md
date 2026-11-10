# Changelog

## [0.2.0](https://github.com/takumi-mr/AegisCMS/compare/plugin-official-clarity-v0.1.0...plugin-official-clarity-v0.2.0) (2026-11-10)


### ⚠ BREAKING CHANGES

* **plugin:** PLUGIN_ABI_VERSION is 2. A <script> written into extra_head, prepend_content or append_content is removed; use PageRenderContext.scripts. official-mermaid script_url is limited to a path on the site or cdn.jsdelivr.net, and a <script> in official-ads custom_head_code no longer works.

### Features

* **plugin:** add official-clarity for Microsoft Clarity heatmaps and recordings ([#297](https://github.com/takumi-mr/AegisCMS/issues/297)) ([246fe7b](https://github.com/takumi-mr/AegisCMS/commit/246fe7b3430067d3f8c04c62db4f181f374ae2df))
* **plugin:** add structured script injection (ABI 2) and drop raw scripts from plugin HTML ([#301](https://github.com/takumi-mr/AegisCMS/issues/301)) ([0a19398](https://github.com/takumi-mr/AegisCMS/commit/0a19398e52b62eed21d0a141a7c10af41058b7ae))
