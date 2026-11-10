# Changelog

## [0.3.0](https://github.com/takumi-mr/AegisCMS/compare/plugin-official-share-buttons-v0.2.1...plugin-official-share-buttons-v0.3.0) (2026-11-10)


### ⚠ BREAKING CHANGES

* **server:** style="" attributes in theme templates, plugin output, and core/html blocks (including inside the sandbox iframe) no longer apply. Use a class with inline_style / a stylesheet / extra_css, or strict_csp = "off" / "report-only".
* **plugin:** PLUGIN_ABI_VERSION is 2. A <script> written into extra_head, prepend_content or append_content is removed; use PageRenderContext.scripts. official-mermaid script_url is limited to a path on the site or cdn.jsdelivr.net, and a <script> in official-ads custom_head_code no longer works.

### Features

* **plugin:** add structured script injection (ABI 2) and drop raw scripts from plugin HTML ([#301](https://github.com/takumi-mr/AegisCMS/issues/301)) ([0a19398](https://github.com/takumi-mr/AegisCMS/commit/0a19398e52b62eed21d0a141a7c10af41058b7ae))
* **server:** stop allowing style="" attributes on the public pages ([#308](https://github.com/takumi-mr/AegisCMS/issues/308)) ([93c7d35](https://github.com/takumi-mr/AegisCMS/commit/93c7d3570849ba3fb28d684614fbeabffc933bf2))

## [0.2.1](https://github.com/takumi-mr/AegisCMS/compare/plugin-official-share-buttons-v0.2.0...plugin-official-share-buttons-v0.2.1) (2026-11-04)


### Performance

* **runtime:** add append slots and input projection to on-render-page ([#209](https://github.com/takumi-mr/AegisCMS/issues/209)) ([8cf01b0](https://github.com/takumi-mr/AegisCMS/commit/8cf01b074fb95d885dabd4bd3602a64a7517ea7d))

## [0.2.0](https://github.com/takumi-mr/AegisCMS/compare/plugin-official-share-buttons-v0.1.0...plugin-official-share-buttons-v0.2.0) (2026-11-02)


### Features

* **core:** apply versioning to the core, SDKs and plugins with release-please ([#194](https://github.com/takumi-mr/AegisCMS/issues/194)) ([a0ef60b](https://github.com/takumi-mr/AegisCMS/commit/a0ef60b26b505dab7b7f8ed77ef224835ea06939))
* **plugin:** add official reading time and post navigation plugins ([#192](https://github.com/takumi-mr/AegisCMS/issues/192)) ([b9f7506](https://github.com/takumi-mr/AegisCMS/commit/b9f7506fc92943bc9daab7dc7a42fe908a5fbb5b))
