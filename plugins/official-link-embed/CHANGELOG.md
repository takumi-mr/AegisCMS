# Changelog

## [0.2.0](https://github.com/takumi-mr/AegisCMS/compare/plugin-official-link-embed-v0.1.0...plugin-official-link-embed-v0.2.0) (2026-11-05)


### ⚠ BREAKING CHANGES

* **server:** plugin HTTP requests to ports other than 80/443/8080 or to internal IP addresses are now refused, and redirects are followed (with the allowed domains checked on every hop) instead of being returned as 3xx.

### Features

* **server:** oEmbed provider, stricter plugin network policy and official-link-embed ([#226](https://github.com/takumi-mr/AegisCMS/issues/226)) ([a907365](https://github.com/takumi-mr/AegisCMS/commit/a9073653879b7f484d7036c6c6055521ce8addd4))
