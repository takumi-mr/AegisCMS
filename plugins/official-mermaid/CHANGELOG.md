# Changelog

## [0.2.0](https://github.com/takumi-mr/AegisCMS/compare/plugin-official-mermaid-v0.1.0...plugin-official-mermaid-v0.2.0) (2026-11-10)


### ⚠ BREAKING CHANGES

* **plugin:** PLUGIN_ABI_VERSION is 2. A <script> written into extra_head, prepend_content or append_content is removed; use PageRenderContext.scripts. official-mermaid script_url is limited to a path on the site or cdn.jsdelivr.net, and a <script> in official-ads custom_head_code no longer works.
* **plugin:** Mermaid diagrams moved to official-mermaid. A site that lists only official-code-enhancer in [plugins].enabled must add official-mermaid to keep its diagrams. config/default.toml enables both.

### Features

* **plugin:** add structured script injection (ABI 2) and drop raw scripts from plugin HTML ([#301](https://github.com/takumi-mr/AegisCMS/issues/301)) ([0a19398](https://github.com/takumi-mr/AegisCMS/commit/0a19398e52b62eed21d0a141a7c10af41058b7ae))
* **plugin:** split Mermaid out of official-code-enhancer into official-mermaid ([#279](https://github.com/takumi-mr/AegisCMS/issues/279)) ([7104a19](https://github.com/takumi-mr/AegisCMS/commit/7104a1931ad546e407d3b004546df3dca26e4edc))
