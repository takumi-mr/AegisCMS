# Changelog

## [0.3.0](https://github.com/takumi-mr/AegisCMS/compare/v0.2.0...v0.3.0) (2026-11-04)


### ⚠ BREAKING CHANGES

* **server:** [contact.captcha] and [newsletter.captcha] are removed. Move them to [captcha] and list the targets.

### Features

* **admin:** add initial setup form and auto-detection in LoginView ([#205](https://github.com/takumi-mr/AegisCMS/issues/205)) ([ef54d15](https://github.com/takumi-mr/AegisCMS/commit/ef54d1525f426ee7c15a0a4db856c7938a785bf8))
* **admin:** add missing storybook stories and expand e2e tests ([#198](https://github.com/takumi-mr/AegisCMS/issues/198)) ([5736b7c](https://github.com/takumi-mr/AegisCMS/commit/5736b7c812e6a942e4c3d8f20b2815916667c877))
* **deploy:** publish docker image to ghcr and update deploy scripts ([#201](https://github.com/takumi-mr/AegisCMS/issues/201)) ([8f5626c](https://github.com/takumi-mr/AegisCMS/commit/8f5626c024756015a96931ad2a168f13b1a6626e))
* **plugin:** add a block plugin template and tutorial, and smooth the plugin dev path ([#212](https://github.com/takumi-mr/AegisCMS/issues/212)) ([fee6a39](https://github.com/takumi-mr/AegisCMS/commit/fee6a390cd0071cb074a50d21ce7b652867c03ed))
* **plugin:** improve developer onboarding with scaffolding and lightweight dev server ([#215](https://github.com/takumi-mr/AegisCMS/issues/215)) ([2e0acce](https://github.com/takumi-mr/AegisCMS/commit/2e0acce7a24f3c81b8b5f0603b2ca68c4aa53c0e))
* **runtime:** scale Wasm fuel dynamically by input payload size and log hook failures ([#218](https://github.com/takumi-mr/AegisCMS/issues/218)) ([7695309](https://github.com/takumi-mr/AegisCMS/commit/7695309ef07b6d01ca9a89d68e555b1e5ff82e04))
* **server:** add a review step with a contributor role and a pending status ([#210](https://github.com/takumi-mr/AegisCMS/issues/210)) ([3dc1af6](https://github.com/takumi-mr/AegisCMS/commit/3dc1af6604070b75375ca1a569c828854c459b1b))
* **server:** add aegis init command and initial setup service ([#202](https://github.com/takumi-mr/AegisCMS/issues/202)) ([39532a7](https://github.com/takumi-mr/AegisCMS/commit/39532a7dbc2a5f91d3de9823ba8f734e3702859c))
* **server:** add web installer setup API and standalone setup page ([#204](https://github.com/takumi-mr/AegisCMS/issues/204)) ([c473611](https://github.com/takumi-mr/AegisCMS/commit/c4736116a7f9660fe4a54319dbcb4375c8b010a2))
* **server:** make public identifiers configurable and drop AegisCMS strings ([#200](https://github.com/takumi-mr/AegisCMS/issues/200)) ([4264a54](https://github.com/takumi-mr/AegisCMS/commit/4264a546ef74566e74c74d8ad10f74bb34147a8a))
* **server:** unify CAPTCHA into a shared [captcha] with per-target switches ([#207](https://github.com/takumi-mr/AegisCMS/issues/207)) ([6106aec](https://github.com/takumi-mr/AegisCMS/commit/6106aec420ca05796f442cdb459fe05baf58747a))
* **theme-builder:** improve theme authoring with scaffolding and lightweight dev server ([#216](https://github.com/takumi-mr/AegisCMS/issues/216)) ([1e14506](https://github.com/takumi-mr/AegisCMS/commit/1e145066a787522b2c40ed22a338d7e454926078))


### Bug Fixes

* **server:** reject absolute storage keys to close editor-role file write/delete escape ([#203](https://github.com/takumi-mr/AegisCMS/issues/203)) ([25f2a5b](https://github.com/takumi-mr/AegisCMS/commit/25f2a5b6f40fc2922740311def2223e7575c65b5))


### Performance

* **bench:** remeasure pure database performance excluding audit sync writes ([#214](https://github.com/takumi-mr/AegisCMS/issues/214)) ([dd1079d](https://github.com/takumi-mr/AegisCMS/commit/dd1079db46ab2803465267e13c8f56bf21d992d0))
* **ci:** optimize CI pipeline with caching and selective docker build ([#217](https://github.com/takumi-mr/AegisCMS/issues/217)) ([44c8bd3](https://github.com/takumi-mr/AegisCMS/commit/44c8bd37764503c9097be0a4c3282de0779b325f))
* **runtime:** add append slots and input projection to on-render-page ([#209](https://github.com/takumi-mr/AegisCMS/issues/209)) ([8cf01b0](https://github.com/takumi-mr/AegisCMS/commit/8cf01b074fb95d885dabd4bd3602a64a7517ea7d))
* **runtime:** cache on-render-page results and speed up reading-time ([#213](https://github.com/takumi-mr/AegisCMS/issues/213)) ([67c5218](https://github.com/takumi-mr/AegisCMS/commit/67c5218a342bbd94c172a73478a3ddab4c819730))
* **server:** list published content in SQL so pages do not scale with article count ([#221](https://github.com/takumi-mr/AegisCMS/issues/221)) ([94eaad8](https://github.com/takumi-mr/AegisCMS/commit/94eaad8d78e10b90d67068c1eac0bc9542043c9a))
* switch release profile from Fat LTO to Thin LTO ([#220](https://github.com/takumi-mr/AegisCMS/issues/220)) ([1c68226](https://github.com/takumi-mr/AegisCMS/commit/1c682267372bde14fded5d81b30e81f822fb675d))

## [0.2.0](https://github.com/takumi-mr/AegisCMS/compare/v0.1.0...v0.2.0) (2026-11-02)


### Features

* **core:** apply versioning to the core, SDKs and plugins with release-please ([#194](https://github.com/takumi-mr/AegisCMS/issues/194)) ([a0ef60b](https://github.com/takumi-mr/AegisCMS/commit/a0ef60b26b505dab7b7f8ed77ef224835ea06939))
* **plugin:** add official reading time and post navigation plugins ([#192](https://github.com/takumi-mr/AegisCMS/issues/192)) ([b9f7506](https://github.com/takumi-mr/AegisCMS/commit/b9f7506fc92943bc9daab7dc7a42fe908a5fbb5b))
