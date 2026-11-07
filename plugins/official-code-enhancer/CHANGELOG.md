# Changelog

## [0.5.0](https://github.com/takumi-mr/AegisCMS/compare/plugin-official-code-enhancer-v0.4.0...plugin-official-code-enhancer-v0.5.0) (2026-11-07)


### ⚠ BREAKING CHANGES

* **plugin:** Mermaid diagrams moved to official-mermaid. A site that lists only official-code-enhancer in [plugins].enabled must add official-mermaid to keep its diagrams. config/default.toml enables both.

### Features

* **plugin:** split Mermaid out of official-code-enhancer into official-mermaid ([#279](https://github.com/takumi-mr/AegisCMS/issues/279)) ([7104a19](https://github.com/takumi-mr/AegisCMS/commit/7104a1931ad546e407d3b004546df3dca26e4edc))

## [0.4.0](https://github.com/takumi-mr/AegisCMS/compare/plugin-official-code-enhancer-v0.3.0...plugin-official-code-enhancer-v0.4.0) (2026-11-07)


### Features

* **plugin:** choose the code theme in the code enhancer's plugin settings ([#267](https://github.com/takumi-mr/AegisCMS/issues/267)) ([309fea7](https://github.com/takumi-mr/AegisCMS/commit/309fea77322ef34bac792eb44c944944781f83d7))

## [0.3.0](https://github.com/takumi-mr/AegisCMS/compare/plugin-official-code-enhancer-v0.2.0...plugin-official-code-enhancer-v0.3.0) (2026-11-04)


### Features

* **server:** make public identifiers configurable and drop AegisCMS strings ([#200](https://github.com/takumi-mr/AegisCMS/issues/200)) ([4264a54](https://github.com/takumi-mr/AegisCMS/commit/4264a546ef74566e74c74d8ad10f74bb34147a8a))


### Performance

* **runtime:** add append slots and input projection to on-render-page ([#209](https://github.com/takumi-mr/AegisCMS/issues/209)) ([8cf01b0](https://github.com/takumi-mr/AegisCMS/commit/8cf01b074fb95d885dabd4bd3602a64a7517ea7d))

## [0.2.0](https://github.com/takumi-mr/AegisCMS/compare/plugin-official-code-enhancer-v0.1.0...plugin-official-code-enhancer-v0.2.0) (2026-11-02)


### Features

* **core:** apply versioning to the core, SDKs and plugins with release-please ([#194](https://github.com/takumi-mr/AegisCMS/issues/194)) ([a0ef60b](https://github.com/takumi-mr/AegisCMS/commit/a0ef60b26b505dab7b7f8ed77ef224835ea06939))
