# Changelog

## [0.2.0](https://github.com/takumi-mr/AegisCMS/compare/sdk-assemblyscript-v0.1.1...sdk-assemblyscript-v0.2.0) (2026-11-05)


### ⚠ BREAKING CHANGES

* **server:** plugin HTTP requests to ports other than 80/443/8080 or to internal IP addresses are now refused, and redirects are followed (with the allowed domains checked on every hop) instead of being returned as 3xx.

### Features

* **server:** oEmbed provider, stricter plugin network policy and official-link-embed ([#226](https://github.com/takumi-mr/AegisCMS/issues/226)) ([a907365](https://github.com/takumi-mr/AegisCMS/commit/a9073653879b7f484d7036c6c6055521ce8addd4))

## [0.1.1](https://github.com/takumi-mr/AegisCMS/compare/sdk-assemblyscript-v0.1.0...sdk-assemblyscript-v0.1.1) (2026-11-04)


### Performance

* **runtime:** add append slots and input projection to on-render-page ([#209](https://github.com/takumi-mr/AegisCMS/issues/209)) ([8cf01b0](https://github.com/takumi-mr/AegisCMS/commit/8cf01b074fb95d885dabd4bd3602a64a7517ea7d))
