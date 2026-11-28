**English** | [日本語](README.ja.md)

# Test plugins

Small Wasm plugins for AegisCMS's own tests. They are **not shipped** (not in the Docker image or the release archives) and **not in the store**. The core's tests use them instead of the official plugins, which are distributed through the [package store](../docs/store.md).

| Plugin        | What it does                                                                                                                                                          | Used by                                                                                                          |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `test-page`   | Marks articles with an inline script (`on-render-page`, with a CSP host), draws `test-page/box` with its own style (`render-blocks`), declares settings of every type | Plugin switching and settings validation (`handlers/plugin.rs`), E2E (`plugins.spec.ts`, `csp.spec.ts`), pentest |
| `test-shop`   | A shipping fee and one coupon (`calculate-order`), counts the coupon's uses in its KV store (`on-order-status-changed`). Its settings are not declared                | Checkout (`handlers/shop_tests.rs`), E2E (`plugins.spec.ts`)                                                     |
| `test-notify` | Posts staff notifications (`send-notification`) and listed order statuses (`on-order-status-changed`) to `webhook_url`                                                | Notification channel (`notify_plugins.rs`), checkout (`handlers/shop_tests.rs`)                                  |

The built `plugin.wasm` files are committed, like the official plugins. After changing a plugin's source, rebuild them:

```bash
task plugin:build:test   # needs: rustup target add wasm32-unknown-unknown
```

The E2E server (`admin/e2e/aegis.e2e.toml`) and the pentest (`docker/pentest/pentest.toml`) read this directory as `bundled_plugins_dir`.
