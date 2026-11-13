[← Back to the documentation index](README.md)

**English** | [日本語](plugin_hooks.ja.md)

# AegisCMS Plugin Hooks & Host API Reference

A list of the hooks, manifest items, and host APIs available to Wasm plugins.
For the plugin development steps themselves, see the [Official Wasm plugin development guide](./plugin_sdk_guide.md).

Related sources:

- Hook dispatch: [`crates/aegis-runtime/src/plugin.rs`](../crates/aegis-runtime/src/plugin.rs) (`PluginSupervisor`)
- Wasm calls: [`crates/aegis-runtime/src/wasm/instance.rs`](../crates/aegis-runtime/src/wasm/instance.rs)
- Host API: [`crates/aegis-runtime/src/wasm/engine.rs`](../crates/aegis-runtime/src/wasm/engine.rs)
- Permission checks: [`crates/aegis-runtime/src/guard.rs`](../crates/aegis-runtime/src/guard.rs)
- Common ABI specification: [`docs/plugin_abi_specification.md`](./plugin_abi_specification.md)
- Rust SDK: [`crates/aegis-plugin-sdk`](../crates/aegis-plugin-sdk)
- AssemblyScript template: [`sdk/templates/assemblyscript`](../sdk/templates/assemblyscript)
- TinyGo template: [`sdk/templates/tinygo`](../sdk/templates/tinygo)

---

## 1. Hook List

| Hook name in the manifest | Wasm export name | SDK trait method | Input | Output | Status |
| --- | --- | --- | --- | --- | --- |
| `transform-content-ast` | `transform_content_ast` | `Plugin::transform_content_ast` | `BlockNode[]` | `BlockNode[]` | ✅ Implemented |
| `on-before-save` | `on_before_save` | `Plugin::on_before_save` | `PostContext` | `PostContext` | ✅ Implemented (changing `title` / `slug` / body, refusing saves) |
| `on-user-authenticated` | `on_user_authenticated` | `Plugin::on_user_authenticated` | `UserAuthContext` | `UserAuthContext` | ✅ Implemented (role mapping, supplementing attributes, refusing login) |
| `calculate-order` | `calculate_order` | `Plugin::calculate_order` | `OrderContext` | `OrderContext` | ✅ Implemented (adding shipping, tax, discounts; refusing orders; [E-commerce](commerce.md)) |
| `on-order-status-changed` | `on_order_status_changed` | `Plugin::on_order_status_changed` | `OrderStatusChange` | None | ✅ Implemented (notification of order creation and status changes) |
| `send-notification` | `send_notification` | `Plugin::send_notification` | `NotificationMessage` | `Delivery` | ✅ Implemented (a notification delivery channel; Slack, SMS, etc.; [Notifications](notifications.md#5-adding-delivery-methods)) |
| `on-content-published` | `on_content_published` | `Plugin::on_content_published` | `PublishedContent` | None | ✅ Implemented (notification when content is published, such as by scheduled publication) |
| `check-comment` | `check_comment` | `Plugin::check_comment` | `CommentSubmission` | `CommentVerdict` | ✅ Implemented (comment moderation; spam judgment, awaiting approval, refusing posts; [Comments](comments.md)) |
| `render-blocks` | `render_blocks` | `Plugin::render_blocks` | `BlockNode[]` (this plugin's blocks) | `Option<String>[]` (HTML) | ✅ Implemented (rendering custom body blocks; [1.10](#110-render-blocks)) |

- To enable a hook, you must **list the hook name (kebab-case) in the manifest's `capabilities.hooks`**. Hooks not listed are not called even if the Wasm side exports the function.
- Conversely, if a hook is listed in the manifest but the corresponding function is not exported, the input is passed on as is (it is not an error).
- The SDK's `#[aegis_plugin]` macro (or `register_plugin!`) always generates the exports for all hooks. Methods you did not implement fall back to the trait's default implementation (returning the input as is).

### 1.1 Execution Timing and Order

`transform-content-ast` / `on-before-save` are called by **the content creation API `POST /api/v1/contents/{content_type}`**, and only `transform-content-ast` by **the editor's preview API `POST /api/v1/preview`** (they are not called by list and get APIs or the display of public pages, which render saved blocks with steps 4 and 5 only). The processing order is as follows.

```
The request's blocks
  │
  ├─ 1. Built-in AST pipeline (HeadingIdTransformer: assigns ids to headings)
  ├─ 2. transform-content-ast  … chained through all plugins in order
  ├─ 3. on-before-save         … chained through all plugins in order (changing title / slug / body, refusing saves)
  ├─ 4. HTML rendering (HtmlRenderer)
  ├─ 5. Streaming HTML rewriter ([security] settings)
  └─ 6. Save to the database
```

- The preview API runs steps 1, 2, 4, and 5 (no saving and no `on-before-save`) and returns the same HTML as when saved and displayed on the public page. For the whole theme-applied page (`page_html`), the `on-render-page` hook ([Theme guide](theme_and_publishing_guide.md)) is also called on every preview.
- When a saved article is opened and re-saved or previewed, `transform-content-ast` is applied again to the saved (transformed) blocks. A plugin that adds blocks should be implemented so as not to add again if already added (idempotent).
- With multiple plugins, the previous plugin's output becomes the next plugin's input.
- The plugin execution order is the order of the enabled list (the order written in `[plugins].enabled` in the config file; ones enabled in Admin Studio come last) ([Configuration reference](configuration.md#plugins--enabling-plugins), [7.1](#71-toggling-on-and-off-from-admin-studio)).

### 1.2 `transform-content-ast`

Transforms the body's block AST. Use it for adding anchors to headings, expanding shortcodes, adding and removing blocks, and so on.

```rust
fn transform_content_ast(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>>;
```

- Input: the block array after the built-in pipeline (an array of top level only; child blocks go in `children`)
- Output: the transformed block array. **This is saved and rendered to HTML as is.**
- Behavior on error: if a host-side error occurs in any plugin (trap, timeout, fuel exhaustion, returning invalid JSON, etc.), **the result of this hook as a whole is discarded** and saving continues with the result of step 1 (the request does not fail). In the preview, the same case is returned as `warnings` and shown in the editor (it is not recorded in the audit log).
- **Do not give it side effects**: the editor's preview calls this hook on every edit (each time input pauses for 300ms). Put processing you want done only once at save time, such as KVS writes and external notifications, in `on-before-save`.

### 1.3 `on-before-save`

Receives the article information right before saving. Use it for validation (refusing saves), correcting saved values, external notifications, and so on. It is not called in the preview, so put side effects you want done only once at save time here.

```rust
fn on_before_save(&mut self, post: PostContext) -> Result<PostContext>;
```

- Input: `PostContext` (`blocks_json` is the content after `transform-content-ast`)
- Output: `PostContext`. It becomes the next plugin's input, and the last plugin's output is saved.
  - **Changes to `title` / `slug` / `blocks_json` are reflected in the save** (the API's response also has the changed values. Admin Studio shows the saved contents in the editor after saving).
  - Changes to `id` and `author_id` are ignored (only a warning log). A plugin cannot swap the author.
  - If `blocks_json` is changed, the built-in AST pipeline (assigning heading ids) is applied again before saving. HTML rendering and the security rewriter apply as usual. `transform-content-ast` is not rerun.
  - Returning an empty `slug`, or a `blocks_json` that cannot be parsed as `BlockNode[]`, aborts the save as a plugin error (an empty string for `blocks_json` is an empty body).
- **Refusing a save**: returning `Err(...)` from the trait method in the SDK aborts the save, and the API returns `422 Unprocessable Entity` with `{"error": "Rejected by plugin: <plugin name>: <message>"}` (shown as an error toast in Admin Studio). For messages to show users, use `PluginError::Rejected` (other variants get the kind prefixed, like `Execution error: ...`). Only `content.create` (`denied`) is recorded in the audit log.

  ```rust
  fn on_before_save(&mut self, post: PostContext) -> Result<PostContext> {
      if post.title.trim().is_empty() {
          return Err(PluginError::Rejected("title is required".to_string()));
      }
      Ok(post)
  }
  ```

- **Plugin failure**: a panic (Wasm trap), timeout, fuel exhaustion, memory limit exceeded, returning invalid JSON, and the like also abort the save, and the API returns `500 Internal Server Error` with `{"error": "Plugin hook failed: ..."}`. `plugin.error` and `content.create` (`denied`) are recorded in the audit log.

### 1.4 `on-user-authenticated`

Called at login (`POST /api/v1/auth/session`) **after** the authentication provider has verified the credentials and **before** the session is issued. Use it for role mapping from an IdP's groups, supplementing display names and attributes, restricting login by domain or time of day, login notifications, and so on. Verification of the credentials themselves (checking passwords and IdP signatures) is not opened to plugins.

```rust
fn on_user_authenticated(&mut self, ctx: UserAuthContext) -> Result<UserAuthContext>;
```

- Input: `UserAuthContext` ([2.3](#23-userauthcontext)). `role` is the role decided by the user account (local authentication) or the provider.
- Output: `UserAuthContext`. It becomes the next plugin's input, and the last plugin's output issues the session.
  - Changes to `role` / `display_name` / `email` / `metadata` are reflected in the session. `metadata` goes into the session's `metadata` (the keys `provider` / `display_name` / `email` are set by the server and cannot be overwritten).
  - Changes to `provider` and `user_id` are ignored (only a warning log).
  - `role` must be one of `admin` / `editor` / `viewer`; returning anything else makes login fail as a plugin error.
  - When the role is changed, `{ "from": original role, "to": new role }` is recorded in `details.role_mapped` of the audit log `session.create` (`success`).
- **Refusing login**: returning `Err(PluginError::Rejected("..."))` makes the API return `403 Forbidden` with `{"error": "Rejected by plugin: <plugin name>: <message>"}`, and no session is issued. The audit log is `session.create` (`denied`, `details.error = "plugin_rejected"`).
- **Plugin failure**: traps, timeouts, invalid JSON, unknown roles, and the like also make login fail (erring on the safe side). The API returns `500`, and `plugin.error` and `session.create` (`failure`, `details.error = "plugin_failed"`) are recorded in the audit log.
- Neither refusals nor failures count toward the login attempt failure count ([rate limiting](authentication.md#5-security-behavior)) (because the credentials are correct).
- It is called on every login. Keep time-consuming processing such as external notifications within `execution_timeout_ms`.

```rust
fn on_user_authenticated(&mut self, mut ctx: UserAuthContext) -> Result<UserAuthContext> {
    if !ctx.email.ends_with("@example.com") {
        return Err(PluginError::Rejected("only example.com accounts may sign in".to_string()));
    }
    let groups = ctx.metadata.get("groups").and_then(|g| g.as_array());
    if groups.is_some_and(|g| g.iter().any(|v| v == "cms-admins")) {
        ctx.role = "admin".to_string();
    }
    Ok(ctx)
}
```

### 1.5 `calculate-order`

Called for every cart quote (`POST /api/v1/shop/quote`) and order confirmation (`POST /api/v1/shop/checkout`) of e-commerce (`[commerce]`, [E-commerce document](commerce.md)). It adds shipping, tax, and discounts to `adjustments`.

```rust
fn calculate_order(&mut self, ctx: OrderContext) -> Result<OrderContext>;
```

- Input: `OrderContext` ([2.4](#24-ordercontext--orderstatuschange)). The unit prices and subtotals in `lines` are values the server decided from the products, and `adjustments` holds what previous plugins added.
- Output: `OrderContext`. **Only `adjustments` and `metadata` are reflected.** Changing `lines`, `subtotal`, `currency`, the customer, or the coupon code is ignored, and the host recomputes `total` as `subtotal + the sum of adjustments` (a plugin cannot rewrite prices).
- The host puts the plugin name in the `source` of `adjustments` the plugin newly added. Discounts are negative amounts. Orders whose total becomes negative are refused.
- **Refusing an order**: returning `Err(PluginError::Rejected("..."))` makes the quote or order `422`, and the message (without the plugin name) is shown to the customer. Use it for invalid coupons, regions that cannot be delivered to, and so on. A failure (trap, timeout) is `500`.
- It is called on every quote, so if you call an external shipping API or the like with `http::fetch`, make `execution_timeout_ms` larger.

### 1.6 `on-order-status-changed`

Called on order creation (`from_status` is `None`) and on every status change (paid, shipped, completed, cancelled, expired). Use it for integration with Slack, email delivery services, accounting software, and so on.

```rust
fn on_order_status_changed(&mut self, change: OrderStatusChange) -> Result<()>;
```

- It is called **after** the status change is saved, without waiting for a response (in a separate task). The result does not affect the order.
- If it returns `Err` or fails, it is left in the warning log and the audit log `plugin.error` (`resource_type: order`). Calls to other plugins continue.

### 1.7 `send-notification`

Makes a plugin a notification delivery channel ([Notifications](notifications.md)). When the server settings write `"plugin:<plugin name>"` in `[notifications].channels`, it is called on every notification (order receipt and shipping, audit events such as administrator logins and login lockouts). The host has built the subject and body from templates, and the plugin only sends them with `http::fetch` to Slack, Teams, Discord, an SMS vendor's API, and so on.

```rust
fn send_notification(&mut self, message: NotificationMessage) -> Result<Delivery>;

impl Plugin for SlackPlugin {
    fn send_notification(&mut self, message: NotificationMessage) -> Result<Delivery> {
        // Handle only notifications for operators (leave customer ones to email)
        if message.recipient != Recipient::Staff {
            return Ok(Delivery::Skipped);
        }
        let url = config::get("webhook_url")
            .ok_or_else(|| PluginError::Rejected("webhook_url is not configured".into()))?;
        let text = format!("*{}*\n{}", message.subject, message.body);
        let response = http::fetch(&http::Request::post_json(url, &serde_json::json!({ "text": text }))?)?;
        if !response.is_success() {
            return Err(PluginError::Execution(format!("Slack answered {}", response.status)));
        }
        Ok(Delivery::Sent)
    }
}
```

| Return value | How the host treats it |
| --- | --- |
| `Ok(Delivery::Sent)` | Sent. Audit log `notification.send` (`success`, `channel: plugin:<name>`) |
| `Ok(Delivery::Skipped)` | A notification (kind, recipient) this plugin does not handle. Not a failure, and not left in the audit log |
| `Err(PluginError::Execution / Host)` | A transient failure. Retried at intervals up to `[notifications].max_attempts` (timeouts and traps are the same) |
| `Err(PluginError::Rejected / Serialization)` | A failure that would not change on retry (a configuration defect, the other side refused with 4xx, etc.). Not retried |

- If you write in `channels` a plugin that lacks `send-notification` in the manifest's `capabilities.hooks` or is not in `[plugins].enabled`, the server does not start.
- Retries, the sending limit (the audit event's `max_per_hour`), the audit log, and templates are handled by the host. Selecting recipients (only for operators, only `audit.`, etc.) is done by the plugin with `Skipped`.
- Include the destination domain in the manifest's `capabilities.network.allowed_domains`. The hook's `execution_timeout_ms` includes the communication time.
- With `aegis-server notify test --staff`, you can send a test notification for operators to the plugin.

### 1.8 `on-content-published`

Called right after content becomes published, such as by scheduled publication. Use it for calling external webhooks, auto-posting to SNS, triggering rebuilds of static site generators, and so on.

```rust
fn on_content_published(&mut self, content: PublishedContent) -> Result<()>;
```

- It is called **after** the scheduler (`publish_due_contents`) updates the content to `published` at the scheduled time and records the audit log `content.publish`.
- The result does not affect the content's published state.
- If a plugin returns `Err` (or a trap or timeout occurs), a warning log and the audit log `plugin.error` (`details.hook = "on-content-published"`) are recorded. Calls to other plugins and the publishing process continue as they are.
- Notifying operators (by email, etc.) can be sent automatically by setting `action = "content.publish"` in `[[notifications.audit.rules]]`.

### 1.9 `check-comment`

Called when a comment is posted from the public site and before it is saved ([Comments](comments.md#6-moderating-with-plugins)). Sort comments into awaiting approval or spam with a query to an external spam detection service (Akismet, etc.) or your own banned words.

```rust
fn check_comment(&mut self, comment: CommentSubmission) -> Result<CommentVerdict>;
```

- Return `CommentVerdict::Allow` (keep the host's judgment) / `Hold { reason }` (awaiting approval) / `Spam { reason }` (spam). It can only be stricter than the host's judgment (`CommentSubmission.status`), and cannot change awaiting approval into published.
- `Err(PluginError::Rejected(..))` refuses the post. The message is shown to the poster as is (`422`).
- With several plugins, they are called in order and the strictest judgment is used (it stops there once `Spam` is returned).
- Failures such as traps and timeouts put the comment awaiting approval and leave a warning log and the audit log `plugin.error` (`details.hook = "check-comment"`).
- It is not called for posts already judged spam by banned words or link counts, or caught by the honeypot.
- The ABI's output is `{"verdict": "allow" | "hold" | "spam", "reason": "..."}` (returning nothing, or no `verdict`, means `allow`), and to refuse, `{"error": "..."}`.

### 1.10 `render-blocks`

Provides custom blocks that can be placed in the body. Blocks declared in the manifest's `blocks` ([3.1](#31-custom-blocks-blocks)) line up in Admin Studio's toolbar and can be edited with a form of the input fields you declared. The name saved is `<plugin name>/<block name>`, and the input values are `attributes`.

```rust
fn render_blocks(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<Option<String>>>;
```

- **When it is called**: on public pages (articles, static pages, products), feeds, the content API's `rendered_html`, and the editor's preview, **on every display**. For one render, it is called once per plugin, with that plugin's blocks passed together in document order (top-level blocks only).
- **Output**: HTML in the same number and order as the input. Blocks with `None` (or missing ones) show nothing. Make blocks with insufficient or odd input `None`.
- **HTML is sanitized**: after removing `<script>`, `<iframe>`, and the like, `on*` attributes, and URLs such as `javascript:` (an `<iframe>` can remain only from hosts declared in `capabilities.embed.frame_domains`; [3.2](#32-embedding-external-pages-capabilitiesembed)), it is wrapped in `<div class="aegis-plugin-block" data-block="<name>">` and output (the same as the custom HTML block's standard mode). Under the Strict CSP (the default), `style` attributes and inline `<style>` are blocked by the browser, so specify the look with class names and `on-render-page`'s `extra_css` (served at `/assets/plugins.css`). If you need a script, add it with `scripts` of `on-render-page` (structured injection; [ABI specification 5.3.3a](plugin_abi_specification.md#533a-adding-scripts-in-on_render_page-scripts-abi-2)).
- **Look**: they are passed together, so `<style>` can be attached once, to the first block. Making the specificity 0 with `:where()` makes it easier to override in the theme's CSS, but link colors and underlines lose to the theme's body link rules (such as `.prose a`), so use a specificity of about two classes (see `official-blocks`).
- **Failure**: on traps, timeouts, invalid JSON, and the like, that plugin's blocks show nothing and a warning log is left (the page display continues). In the preview it is shown in `warnings`.
- **When the plugin is disabled**: the blocks are saved as is, and Admin Studio shows "Plugin disabled". They are not shown on public pages (they become empty custom elements).
- **Performance**: Wasm is called on every display, so calling an external API with `http::fetch` slows the display by that much. Combine it with CDN caching ([`[cdn]`](configuration.md#217-cdn--on-demand-cache-tag-purge-cdn-edge-cache-integration)), or prepare external data some other way.
- The ABI's input is `{"blocks": [BlockNode, ...]}`, the output is `{"html": [string | null, ...]}`, and failure is `{"error": "..."}`.

```rust
impl Plugin for MyBlocks {
    fn render_blocks(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<Option<String>>> {
        Ok(blocks
            .iter()
            .map(|b| match b.name.as_str() {
                "my-blocks/note" => b
                    .attributes
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(|t| format!("<p class=\"my-note\">{}</p>", escape(t))),
                _ => None,
            })
            .collect())
    }
}
```

Attribute values are what users entered. Always escape them when putting them in HTML (sanitization only removes scripts, and tags that break the layout remain).

---

## 2. Data Types Passed to Hooks

All are passed as JSON through the guest's linear memory.

### 2.1 `PostContext`

| Field | Type | Description |
| --- | --- | --- |
| `id` | string | The newly issued content ID (UUID v4) |
| `title` | string | Title |
| `slug` | string | Slug |
| `author_id` | string | Author ID (an empty string if unspecified) |
| `blocks_json` | string | The block array as a JSON string |

SDK helpers:

| Method | Description |
| --- | --- |
| `PostContext::new(title, slug)` | A constructor for tests |
| `post.blocks()` | Parse `blocks_json` into `Vec<BlockNode>` |
| `post.set_blocks(&blocks)` | Write `Vec<BlockNode>` back to `blocks_json` |

### 2.2 `BlockNode`

```json
{ "name": "core/heading", "attributes": { "level": 2 }, "text": "Heading", "children": [] }
```

| Field | Type | Description |
| --- | --- | --- |
| `name` | string | The block kind (`core/heading`, `core/paragraph`, `core/code`, `core/image`, `core/table`, etc.; for the list and attributes see [Body blocks](blocks.md)) |
| `attributes` | object \| null | Arbitrary attributes. A block saved with attributes omitted is passed as `null` (the SDK reads it as an empty map) |
| `text` | string \| null | Text content |
| `children` | `BlockNode[]` | Child blocks |

SDK helpers:

| Method | Description |
| --- | --- |
| `BlockNode::new(name)` | Create an empty block |
| `BlockNode::paragraph(text)` | `core/paragraph` |
| `BlockNode::heading(level, text)` | `core/heading` (with a `level` attribute) |
| `BlockNode::code(language, code)` | `core/code` (with a `language` attribute) |
| `block.attribute(key)` | Get an attribute |
| `block.set_attribute(key, value)` | Set an attribute |

> [!NOTE]
> The SDK's `BlockNode.attributes` is a `HashMap<String, serde_json::Value>` (the core's `aegis-ast` `BlockNode` uses a single `serde_json::Value`, which is a different type). When creating blocks in a plugin's tests, add with `set_attribute` to `BlockNode::new(name)`, or write something like `HashMap::from([("level".to_string(), json!(2))])`. Putting `json!({...})` straight into `attributes` is a compile error.

### 2.3 `UserAuthContext`

| Field | Type | Description |
| --- | --- | --- |
| `provider` | string | The provider that authenticated (`local` / `oidc` / `saml`). Changes are ignored |
| `user_id` | string | The session's `user_id` (the username for local authentication). Changes are ignored |
| `email` | string | Email address (an empty string if unregistered) |
| `display_name` | string \| null | Display name |
| `role` | string | `admin` / `editor` / `viewer` |
| `metadata` | object | Provider-specific attributes. For OIDC, `issuer`, `sub`, and `claims` (the ID token's claims; usable for mapping `groups`, etc.); for SAML, `issuer`, `name_id`, `name_id_format`, `session_index`, and `attributes` (attribute name → array of values); empty for local authentication |

### 2.4 `OrderContext` / `OrderStatusChange`

All amounts are integers in the currency's smallest unit (yen for JPY, cents for USD).

```json
{
  "order_id": "8f0c… (null during a quote)",
  "order_number": "261023-U6D3F6",
  "currency": "jpy",
  "lines": [
    { "product_id": "…", "slug": "ceramic-mug", "title": "Ceramic Mug", "sku": null,
      "unit_price": 1200, "quantity": 2, "line_total": 2400 }
  ],
  "adjustments": [
    { "kind": "discount", "label": "Coupon WELCOME10", "amount": -540, "code": "WELCOME10", "source": "official-shop-rules" },
    { "kind": "shipping", "label": "Shipping", "amount": 600, "code": null, "source": "official-shop-rules" }
  ],
  "subtotal": 5400,
  "total": 5460,
  "customer_email": "hanako@example.com",
  "customer_name": "Hanako Yamada",
  "shipping_address": { "name": "…", "postal_code": "100-0001", "region": "Tokyo", "city": "Chiyoda",
                        "line1": "1-1 Marunouchi", "line2": "", "phone": "", "country": "JP" },
  "coupon_code": "WELCOME10",
  "status": "pending_payment",
  "metadata": {}
}
```

`OrderStatusChange` is `{ "order": OrderContext, "from_status": "pending_payment" | null, "to_status": "paid", "occurred_at": "RFC 3339" }`. The statuses are `pending_payment` / `paid` / `shipped` / `completed` / `cancelled`.

### 2.5 `NotificationMessage` / `Recipient` / `Delivery` (`send-notification`)

| Field | Type | Description |
| --- | --- | --- |
| `kind` | string | The notification kind (`order.placed` / `order.paid` / `order.shipped` / `order.cancelled` / `audit.<operation>` / `test`) |
| `recipient` | object | `{"type": "staff"}` (operators) or `{"type": "person", "email": "...", "name": "..."}` |
| `subject` | string | The subject (built from a template) |
| `body` | string | The body (plain text) |
| `html` | string (optional) | The HTML body (when `[notifications].html = true`; for channels that send to email delivery APIs; [Notifications 3.1](notifications.md#31-html-email)) |
| `data` | object | The values passed to the template (`order` / `audit` / `site_name` / `site_url` / `admin_url` / `kind`, etc.; [Notifications 3](notifications.md#3-templates)) |

`Delivery` is `Sent` (sent) or `Skipped` (not handled).

### 2.6 `PluginError` (SDK)

| Variant | Use |
| --- | --- |
| `PluginError::Rejected(String)` | Does not accept the input (refusing a save in `on_before_save`, refusing login in `on_user_authenticated`). The message appears as is in the API's response |
| `PluginError::Execution(String)` | An error in plugin logic |
| `PluginError::Serialization(String)` | A JSON conversion error (converted automatically from `serde_json::Error`) |
| `PluginError::Host(String)` | An error from a host API call |

### 2.7 `PublishedContent` (`on-content-published`)

| Field | Type | Description |
| --- | --- | --- |
| `id` | string | The ID of the published content (UUID v4) |
| `content_type` | string | The schema name (`articles`, `news`, etc.) |
| `title` | string | Title |
| `slug` | string | Slug |
| `published_at` | string | Publication time (RFC 3339) |
| `author_id` | string \| null | Author ID (null if unspecified) |
| `fields` | object | A map of custom field values |

### 2.8 `CommentSubmission` / `CommentVerdict` (`check-comment`)

| Field | Type | Description |
| --- | --- | --- |
| `content_type` / `content_id` | string | The kind and ID of the content the comment is on |
| `content_title` / `content_slug` | string | The content's title and slug |
| `parent_id` | string \| null | The ID of the comment being replied to |
| `author_name` / `author_email` | string | The poster's name and email address (lowercase) |
| `body` | string | The body (plain text) |
| `ip` / `user_agent` | string \| null | The poster's IP address and browser |
| `status` | string | The host's judgment (`approved` / `pending`) |

`CommentVerdict` is one of `{"verdict": "allow"}`, `{"verdict": "hold", "reason": "..."}`, and `{"verdict": "spam", "reason": "..."}` (`reason` is the reason left in the log; optional).

---

## 3. Manifest (`plugin.manifest.json`)

Each plugin is placed in `<name>/` directly under `plugins_dir` (`<name>` is the manifest's `name`) and has a `plugin.manifest.json`. Placing it alone does not load it; adding the name to `[plugins].enabled` enables it ([7. Installation and enabling](#7-installation-and-enabling)).

```json
{
  "name": "my-plugin",
  "version": "0.1.0",
  "entrypoint": "plugin.wasm",
  "capabilities": {
    "network": { "allowed_domains": ["hooks.slack.com"] },
    "storage": { "kv_namespace": "my_plugin_state" },
    "hooks": ["on-before-save", "transform-content-ast"]
  },
  "resource_limits": {
    "max_memory_mb": 16,
    "max_fuel_units": 100000000,
    "execution_timeout_ms": 250
  }
}
```

| Key | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | string | ✅ | The plugin's identifier. Starts with a lowercase letter or digit, and only lowercase letters, digits, `-`, and `_` (up to 64 characters). Used for the installation directory name, `[plugins].enabled`, and the log tag (`[Wasm:<name>]`) |
| `version` | string | ✅ | Version |
| `entrypoint` | string | ✅ | The path of the Wasm binary (relative to the manifest) |
| `display_name` / `description` / `author` / `license` / `tags` / `homepage` / `repository` / `min_aegis_version` | | | Metadata for stores and list display (the same items as a theme's `theme.json`; `author` is a string or `{"name", "email", "url"}`). Does not affect behavior |
| `capabilities.network.allowed_domains` | string[] | | Domains allowed by `net::is_allowed`. Exact or subdomain match |
| `capabilities.storage.kv_namespace` | string | | The KVS namespace. If undeclared, `kv::set` is a permission error |
| `capabilities.hooks` | string[] | | Hook names to enable ([1. Hook list](#1-hook-list)) |
| `capabilities.csp.script_domains` / `style_domains` / `connect_domains` / `frame_domains` / `img_domains` | string[] | | Hosts of external scripts, CSS, connections, frames, and images. While enabled, the server adds them to the CSP's `script-src` / `style-src` / `connect-src` / `frame-src` / `img-src`. **A plugin that loads `<script>` from a CDN or the like declares that host** (without the declaration, the default CSP blocks it). Host names only (no scheme). Shown in "Capabilities & permissions" in Admin Studio's plugin list |
| `capabilities.inline_scripts` | boolean | `false` | A declaration for adding inline scripts with `scripts` (structured injection) of `on-render-page`. Without the declaration the host drops them. The hosts of external scripts are in `script_domains` above ([ABI specification 5.3.3a](plugin_abi_specification.md#533a-adding-scripts-in-on_render_page-scripts-abi-2)) |
| `capabilities.embed.frame_domains` | string[] | | The source hosts of `<iframe>`s that can be embedded in custom blocks ([3.2](#32-embedding-external-pages-capabilitiesembed)) |
| `blocks` | object[] | | The declaration of custom blocks that can be placed in the body ([3.1](#31-custom-blocks-blocks)) |
| `settings` | object[] | | The declaration of settings items. They appear as input fields in Admin Studio's plugin settings and have their values validated on save ([3.3](#33-settings-items-settings)) |
| `resource_limits` | object | | Omitted items (or the whole) are filled in by the server settings `[runtime].default_*` (below) |

If an enabled plugin's manifest cannot be read, `name` differs from the directory name, the file for `entrypoint` is missing, or it imports a function the host does not have ([ABI specification](plugin_abi_specification.md#3-host-api-import-specification-the-host-module)), **the server does not start**. This is to prevent a plugin that implements access restrictions and the like from silently dropping off while the server keeps running. Defects in plugins that are not enabled are shown in warning logs and `aegis-server plugin list`.

### 3.1 Custom Blocks (`blocks`)

Declares blocks rendered by the `render-blocks` hook ([1.10](#110-render-blocks)). A manifest that has `blocks` but no `render-blocks` in `capabilities.hooks` cannot be loaded.

```json
"capabilities": { "hooks": ["render-blocks"] },
"blocks": [
  {
    "name": "button",
    "label": { "en": "Button", "ja": "ボタン" },
    "description": { "en": "A link styled as a button", "ja": "ボタンの形のリンク" },
    "fields": [
      { "name": "label", "type": "string", "label": { "en": "Label", "ja": "ボタンの文字" }, "required": true },
      { "name": "url", "type": "url", "label": "URL", "required": true, "placeholder": "https://" },
      { "name": "style", "type": "select", "label": "Style", "default": "primary",
        "options": [{ "value": "primary", "label": "Filled" }, { "value": "secondary", "label": "Outline" }] }
    ]
  }
]
```

| Key | Description |
| --- | --- |
| `name` | The block name (the same form as plugin names). Saved in the body as `<plugin name>/<name>` |
| `label` / `description` | The name and description shown in Admin Studio. A string, or per-language `{"en": ..., "ja": ...}` (used in the order Admin Studio's language → English → the first one) |
| `fields[].name` | The attribute name (starts with a letter, then alphanumerics and `_`). The value is `attributes.<name>` |
| `fields[].type` | `string` (one line) / `text` (multi-line) / `url` / `number` / `boolean` / `select` (requires `options`) / `image` (a URL; can be chosen via upload or the media library) / `list` (repeating items that can be added; below) |
| `fields[].label` / `placeholder` / `help` | The input field's name, placeholder, and description (can be per-language like `label`) |
| `fields[].required` | When empty, a warning is shown in the editor (saving is not stopped) |
| `fields[].default` | The initial value of a newly placed block |
| `fields[].fields` | The input fields of one item of a `list` (the same form as `fields[]` above; a `list` cannot be placed inside a `list`). Can be written only on `list` |
| `fields[].max_items` | The upper limit of a `list`'s item count (1–100; 100 if omitted). Can be written only on `list` |
| `refreshable` | If `true`, a "Refetch" button appears in the block's form. Pressing it adds the attribute `refresh: true` to the block and runs it through `transform-content-ast` (without saving), and updates the form with the attributes returned. The plugin removes `refresh` once it has rebuilt the attributes (see `official-link-embed`) |

The types of values the Admin Studio form puts in `attributes` are as follows. Blocks saved from the API or Markdown can hold arbitrary JSON, so plugins should tolerate type differences (see `attr` in `official-blocks` and `sdk/templates/rust-block`).

| `fields[].type` | Value in `attributes` | Notes |
| --- | --- | --- |
| `string` / `text` / `url` / `image` | A string | If the input is empty, the key is absent (or an empty string) |
| `select` | A string (`options[].value`) | If not `required`, an empty string when none is chosen. Numeric choices (such as heading levels) also arrive as **strings** like `"3"` |
| `number` | A JSON number | If emptied, the key is absent. A numeric `default` goes in as is |
| `boolean` | `true` / `false` | If never touched, the key is absent (or the `default` value if there is one) |
| `list` | An array of objects (values keyed by the `name`s of `fields[].fields`). When there are no items, an empty array or the key is absent | Items can be added, removed, and reordered. `required` means "at least one item has something entered". `required` inside an item shows a warning for that item. Items with only `select` and `boolean` count as "empty". The types of items' values are the same as the rows above |

If there is a defect in the declaration (name format or duplicates, a `select` without options, a `list` without `fields` or a nested `list`, an out-of-range `max_items`, an unknown `type`), the server that enabled that plugin does not start.

`list` is for blocks that line up items of the same shape, such as FAQs, galleries, and steps.

```json
{ "name": "items", "type": "list", "label": "Questions", "required": true, "max_items": 30,
  "fields": [
    { "name": "question", "type": "string", "label": "Question", "required": true },
    { "name": "answer", "type": "text", "label": "Answer" }
  ] }
```

The attributes saved are `{"items": [{"question": "...", "answer": "..."}, ...]}`. Plugins should read on the assumption that an array element may not be an object and keys may be missing (arbitrary JSON can be saved from the API and Markdown).

### 3.2 Embedding External Pages (`capabilities.embed`)

To embed an external page such as a video player into a custom block with an `<iframe>`, declare the source host.

```json
"capabilities": {
  "hooks": ["render-blocks"],
  "embed": { "frame_domains": ["www.youtube-nocookie.com", "player.vimeo.com"] }
}
```

- **Sanitization**: only for that plugin's blocks' HTML, `<iframe>`s whose `src` is `https://<declared host>/...` remain. Hosts are exact matches (URLs with a subdomain, user info `user@`, or a port are not allowed). The only attributes kept are `src`, `title`, `width`, `height`, `allow`, `allowfullscreen`, `loading`, `referrerpolicy`, `sandbox`, `class`, `style`, and `frameborder` (`srcdoc`, `on*`, and the like are removed). `<iframe>`s of undeclared hosts, `<iframe>`s in other plugins' blocks, and `<script>` are removed as before.
- **CSP**: the hosts declared by enabled plugins are added to the `frame-src` of every response's `Content-Security-Policy` as `https://<host>`. If the CSP has `frame-src`, it is added there; otherwise a `frame-src` is added with the value of `child-src` (or `default-src` if absent) that applies to iframes. A CSP where an administrator wrote `frame-src 'none'` or `*`, and a CSP that does not restrict iframes, are left as is. This addition continues even if you change the CSP in Admin Studio's **Settings › Headers**.
- **How to write**: host names only (`www.youtube-nocookie.com`). Two or more labels of lowercase letters, digits, and `-` joined with `.`. Schemes, paths, ports, and wildcards cannot be written. It cannot be declared for a plugin without `blocks`.
- Rather than embedding HTML written by users as is, extract the ID from the entered URL and build the player URL yourself (see `official-blocks`'s video block).
- The declared hosts are shown in "Capabilities & permissions" in Admin Studio's plugin list.

### 3.3 Settings Items (`settings`)

Declares input fields for the plugin's settings values (the values read with `config::get`). Declared items appear in Admin Studio's **Settings › Plugins** "Settings" screen as input fields suited to the type (choices, switches, numbers, colors, etc.). Keys without a declaration remain in the list of keys and values as before.

```json
"settings": [
  {
    "key": "position",
    "type": "select",
    "label": { "en": "Position", "ja": "位置" },
    "default": "right",
    "options": [
      { "value": "right", "label": { "en": "Right", "ja": "右" } },
      { "value": "left", "label": { "en": "Left", "ja": "左" } }
    ]
  },
  {
    "key": "top_offset_px",
    "type": "number",
    "label": { "en": "Top offset (px)", "ja": "上からの位置 (px)" },
    "description": { "en": "Space above a heading after jumping to it.", "ja": "見出しへ移動したときの上の余白。" },
    "min": 0, "max": 400, "default": 96
  },
  { "key": "accent_color", "type": "color", "label": "Accent color", "placeholder": "#6366f1" }
]
```

| Key | Required | Description |
| --- | --- | --- |
| `key` | ✅ | The setting's key. Starts with a letter, then only alphanumerics and `_` (up to 64 characters). Read with `config::get(key)` |
| `type` | ✅ | The kind of input field (table below) |
| `label` | ✅ | The input field's name. A string or `{"en": ..., "ja": ...}` (used in the order Admin Studio's language → English → the first one) |
| `description` / `placeholder` | | The description under the field and the faint text. The same form as `label` |
| `default` | | The value the plugin uses when unset. It is only shown as faint text in the field and as a "Default (…)" choice, and **is not saved** (the plugin must use the same default too) |
| `options` | Required for `select` | `[{"value": "...", "label": ...}]`. `value` is non-empty and unique |
| `min` / `max` | | The lower and upper bounds of `number` |
| `max_length` | | The character limit (defaults: `string` 500, `text` 20000, `url` 2000) |

| `type` | Input field | Validation of the saved value |
| --- | --- | --- |
| `string` / `text` | One line / multi-line | The character limit. Control characters are not allowed (line breaks and tabs are allowed for `text`) |
| `url` | One line | `http://`, `https://`, or a relative path starting with `/` (`//` and whitespace are not allowed) |
| `number` | Number | Being a number, and the range of `min` and `max` |
| `boolean` | Default / on / off | `true` or `false` |
| `select` | Choices | One of `options` |
| `color` | Color picker and text | `#hex`, `rgb()`, `hsl()`, `var(--name)`, or a color name (a form that is safe to put straight into CSS) |

- **Values are stored as strings** (`"true"` / `"false"` for `boolean`, decimal for `number`). When reading, the plugin converts the string `config::get` returns into the type.
- **A blank means "unset".** Empty values of declared items are not saved (the key is removed), and the plugin uses its default when `config::get` returns `None`. Validation is also done on the server (`PUT /api/v1/plugins/{name}/settings`), and mistakes are returned as `400 {"code": "invalid_setting", "errors": [{"key", "message"}]}` (nothing is saved). Keys without a declaration are not validated.
- **Values in the config file (`[plugins.settings.<name>]`) are not validated** (they are only read as startup settings). Before putting a value it read into CSS or HTML, the plugin should narrow it itself to only allowed forms. A declaration whose `default` does not pass its own validation, or a `select` with no options, is a manifest defect.
- Working examples are [`official-page-toc`](../plugins/official-page-toc/plugin.manifest.json) and [`official-back-to-top`](../plugins/official-back-to-top/plugin.manifest.json) (adding to the body), and [`official-code-lines`](../plugins/official-code-lines/plugin.manifest.json), [`official-lightbox`](../plugins/official-lightbox/plugin.manifest.json), and [`official-external-links`](../plugins/official-external-links/plugin.manifest.json) (adjusting the body's HTML with scripts).

## 4. Resource Limits (`resource_limits`)

| Key | Value if omitted | Description |
| --- | --- | --- |
| `max_fuel_units` | `[runtime].default_max_fuel_units` (default `100000000`) | The upper limit of Wasm instructions executed per hook call. Exceeding it traps |
| `execution_timeout_ms` | `[runtime].default_timeout_ms` (default `250`) | The upper limit of execution time per hook call (rounded up to a minimum of 50ms) |
| `max_memory_mb` | `[runtime].default_max_memory_mb` (default `16`) | The linear memory limit (MiB, minimum 1). Trying to exceed it with `memory.grow` traps, and a module whose initial memory exceeds the limit fails to instantiate. A Rust `wasm32-unknown-unknown` build uses about 1.1 MiB for initial memory alone, so specify 2 or more. When using the instance pool (the default), keep it at or below `[runtime].pool_max_memory_mb` (default `64`) (exceeding it is an error at load time) |

- They are filled in per item, so you can specify just some, like `{"max_memory_mb": 8}`.
- Exceeding a limit (a trap) is treated as a plugin error (following each hook's behavior on error, and `plugin.error` is recorded in the audit log).

> [!NOTE]
> Hooks run in a new Wasm instance on every call. The plugin struct is also created with `Default::default()` each time, so **in-memory state is not kept between hook calls**.

---

## 5. Host API

The host provides the following functions as imports from the `host` module.

| SDK API | Wasm import | Signature | Required permission |
| --- | --- | --- | --- |
| `log_debug!` / `log_info!` / `log_warn!` / `log_error!` | `host.log` | `(level: i32, ptr: i32, len: i32)` | None |
| `net::is_allowed(url) -> bool` | `host.http_check_allowed` | `(ptr: i32, len: i32) -> i32` | `capabilities.network` |
| `kv::set(key, value) -> Result<()>` | `host.kv_set` | `(k_ptr, k_len, v_ptr, v_len) -> i32` | `capabilities.storage` |
| `kv::set_json(key, &value) -> Result<()>` | `host.kv_set` | Same (the value is turned into a JSON string) | `capabilities.storage` |
| `kv::get(key) -> Result<Option<String>>` / `kv::get_json` | `host.kv_get` | `(k_ptr, k_len) -> i64` | `capabilities.storage` |
| `http::fetch(&Request) -> Result<Response>` | `host.http_fetch` | `(req_ptr, req_len) -> i64` | `capabilities.network` |
| `config::get(key) -> Option<String>` / `config::get_or(key, default)` | `host.config_get` | `(k_ptr, k_len) -> i64` | None |

Functions that return `i64` have the host write the result into a region secured with the guest's `allocate` and return `(ptr << 32) | len` (the SDK reads it and frees it).

### 5.1 Logging (`host.log`)

| `level` | SDK macro | Host-side `tracing` level |
| --- | --- | --- |
| `0` | `log_debug!` | `debug` |
| `1` | `log_info!` | `info` |
| `2` | `log_warn!` | `warn` |
| `3` and above | `log_error!` | `error` |

### 5.2 Network Permission Check (`host.http_check_allowed`)

- Returns `1` if the host name of an `http://` or `https://` URL exactly matches `allowed_domains` or is a subdomain of one, and `0` otherwise. The host name is extracted with the same URL parser as the HTTP client (case-insensitive), and URLs containing user info (`user@host`) are rejected.
- If `capabilities.network` is undeclared, it is always `0`.
- To actually communicate, use `host.http_fetch` ([5.4](#54-http-requests-hosthttp_fetch)).

### 5.3 KVS Write (`host.kv_set`)

| Return value | Meaning | SDK result |
| --- | --- | --- |
| `0` | Success | `Ok(())` |
| `1` | No permission (`capabilities.storage` undeclared) | `Err(PluginError::Host(...))` |
| `2` | An error such as a memory read failure | `Err(PluginError::Host(...))` |

- Keys are stored on the host side in the form `{kv_namespace}:{key}` and are separated from other plugins' namespaces.
- Values are stored in the server's `[cache]` backend under the key `plugin-kv:{kv_namespace}:{key}` (no expiry). With `redis` they remain after a restart and are shared by all servers. With `memory` they exist only in the process and vanish on restart.
- Reading is `host.kv_get` (the SDK's `kv::get` / `kv::get_json`). The return value is the value's `(ptr << 32) | len`; `0` if unset; `-1` for no permission; `-2` for an error.
- Each read and write is independent (an update such as reading, adding 1, and writing can miss counts under concurrent execution).

### 5.4 HTTP Requests (`host.http_fetch`)

```rust
let request = http::Request::post_json("https://hooks.slack.com/services/…", &json!({ "text": "New order" }))?;
let response = http::fetch(&request)?;
if !response.is_success() { /* response.status, response.body */ }
```

- Destinations are only the manifest's `capabilities.network.allowed_domains` (and their subdomains). URLs not allowed are not sent and give an error.
- The methods are `GET` / `POST` / `PUT` / `PATCH` / `DELETE`. Headers such as `Host` / `Content-Length` cannot be specified.
- **The host follows redirects up to 3 times.** At each destination it checks the allowed domains and the destination restrictions below, and moving to a destination that is not allowed is an `Err` (a 3xx is not returned). 303, and 301/302 with a body, change to GET, and when moving to a different host the `Authorization` / `Cookie` headers are not sent.
- **Destination restrictions**: only `http` / `https`, and ports only 443 for `https` and 80 or 8080 for `http`. It does not connect to internal IP addresses (loopback, private, link-local = cloud metadata, etc.) or host names that resolve to them, even if written in the allowed domains.
- **Allowed domains can be added in the site's settings**: domains written in `extra_allowed_domains` (comma-separated, or an array) of `[plugins.settings.<plugin name>]` are handled as added to the manifest's `allowed_domains` (including subdomains). You can add fetch destinations without rebuilding the plugin (it can also be set from Admin Studio's plugin management screen). Administrators set it, so add only when you can trust the plugin.
- Bodies are up to 1 MiB for both sending and receiving. One call takes at most 10 seconds, and up to 8 calls per hook invocation. The hook's `execution_timeout_ms` includes the waiting time for communication, so make it larger (e.g. 5000–10000) for plugins that communicate.
- A failure (a destination not allowed, cannot connect, limit exceeded) is `Err(PluginError::Host(...))`. The host returns JSON of `{"error": "..."}`.

### 5.5 Settings Values (`host.config_get`)

Reads the values of `[plugins.settings.<plugin name>]` in the server's config file (other plugins' settings cannot be read). Strings are returned as is, and numbers, booleans, and arrays as JSON strings.

Dynamic settings values set from Admin Studio's plugin management screen (`PUT /api/v1/plugins/{name}/settings`) are also reflected immediately and override what is written in the config file (they are persisted in the cache and kept after restart).

```toml
[plugins.settings.official-order-notifier]
webhook_url = "env:SLACK_WEBHOOK_URL"   # values starting with env: come from environment variables (do not write API keys in the config file)
notify_on = "pending_payment,paid,cancelled"
```

```rust
let url = config::get("webhook_url");             // Option<String>
let fee: i64 = config::get_or("shipping_fee", 0); // the default if it cannot be read
```

### 5.6 Mocks for Native Tests

When built for a native target such as `cargo test` (`not(target_arch = "wasm32")`), the SDK's host APIs are replaced with mocks.

| API | Mock behavior |
| --- | --- |
| Logging | Outputs `[WasmMock:LEVEL] message` to standard output |
| `net::is_allowed` | Always `true` (does not consult the manifest) |
| `kv::set` / `kv::get` | Stored and read in thread-local memory |
| `config::get` | The value set with `host::mock::set_config(key, value)` |
| `http::fetch` | Records without sending and returns `200` (retrieve with `host::mock::take_http_requests()`) |

`host::mock::reset()` empties the mocks.

Check permission-related behavior by building to Wasm and running on the runtime.

---

## 6. Low-Level ABI (Without the SDK)

| Export | Signature | Required | Description |
| --- | --- | --- | --- |
| `memory` | memory | ✅ | Linear memory |
| `allocate` | `(len: i32) -> i32` | ✅ | Secures a region for the host to write the input JSON into and returns the pointer |
| `transform_content_ast` | `(ptr: i32, len: i32) -> i64` | | Input `BlockNode[]` JSON, return `(ptr << 32) \| len` |
| `on_before_save` | `(ptr: i32, len: i32) -> i64` | | Input `PostContext` JSON, return `(ptr << 32) \| len` |
| `on_user_authenticated` | `(ptr: i32, len: i32) -> i64` | | Input `UserAuthContext` JSON, return `(ptr << 32) \| len` |
| `calculate_order` | `(ptr: i32, len: i32) -> i64` | | Input `OrderContext` JSON, return `(ptr << 32) \| len` (refuse with `{"error"}`) |
| `on_order_status_changed` | `(ptr: i32, len: i32) -> i64` | | Input `OrderStatusChange` JSON. The return is `0` (success) or `{"error"}` |

- If the returned `len` is `0`, the host adopts the input as is.
- To refuse a save in `on_before_save` or a login in `on_user_authenticated`, return `{"error": "<message>"}` instead of the input type (the SDK's `abi::write_error_to_host`). Every object with an `error` key is treated as a refusal.
- If the returned JSON cannot be parsed as the expected type, it is a host-side error (see the behavior on error in [1.2](#12-transform-content-ast) / [1.3](#13-on-before-save)).

## 7. Installation and Enabling

Like themes (`themes/<id>/` + `[theme].active`), plugins also manage "installation" and "enabling" separately. Installation from a future store also assumes this form (extraction into `plugins/<name>/`).

1. **Install**: place `plugin.manifest.json` and `plugin.wasm` in `plugins/<name>/` (`<name>` is the manifest's `name`).
2. **Enable**: press "Enable" in Admin Studio's plugin screen (no restart needed), or add `<name>` to `[plugins].enabled` in the config file and restart the server. Hooks run in the order of the enabled list.

```toml
[plugins]
enabled = ["seo-tools", "slack-notifier"]
```

- The installed plugins and their status can be checked with `aegis-server plugin list` (CLI) or `GET /api/v1/plugins` (admin permission).

### 7.1 Toggling On and Off from Admin Studio

With "Enable" / "Disable" in Admin Studio's plugin screen (`admin` role only), you can switch without restarting the server.

- API: send `{"enabled": true}` / `{"enabled": false}` to `PUT /api/v1/plugins/{name}/enabled`. The response is the same list as `GET /api/v1/plugins`. The audit logs are `plugin.enable` / `plugin.disable`.
- A newly enabled plugin joins **last** in the hook execution order. The order can be changed with "Hook execution order" on the plugin screen (below).
- The toggled list is saved in the DB (`plugins.enabled` in `_aegis_settings`), and **from then on takes precedence over `[plugins].enabled` in the config file** (the startup log shows `Using the plugin list saved from Admin Studio`). To return to the config file's value, use "Return to config file values" (below).
- A plugin that cannot be loaded (not installed, a manifest defect, importing a function the host lacks, etc.) cannot be enabled and returns `400`. Nothing changes in that case.
- A plugin used as `plugin:<name>` in `[notifications].channels` cannot be disabled (`409`; first remove it from `channels`).
- On a server with `[runtime].enable_wasm_plugins = false`, it cannot be switched (`409`).
- When running multiple servers (`[cache].backend = "redis"`), toggling, reordering, and changing settings values are reflected immediately on the other servers too ([7.3](#73-reflection-across-multiple-servers)).
- A plugin's settings values (`PUT /api/v1/plugins/{name}/settings`) are kept whether enabled or disabled and are read from the moment it is enabled.

### 7.2 Reordering Hook Execution and Returning to the Config File's Values

At the top of the plugin screen, "Hook execution order" lists the enabled plugins in execution order (only the `admin` role can operate it). Reordering with the up and down buttons is reflected immediately without a restart and is saved in the DB.

- API: `GET /api/v1/plugins/order` → `{"enabled": [...], "configured": [...], "customized": true}`. `enabled` is the current execution order, `configured` is `[plugins].enabled` in the config file, and `customized` is whether a list changed in Admin Studio is saved in the DB.
- Reordering: `PUT /api/v1/plugins/order` with `{"enabled": ["b", "a"]}`. List the currently enabled plugins exactly once each (missing, extra, or duplicate gives `400`; toggle enabled/disabled with `PUT /api/v1/plugins/{name}/enabled`). The audit log is `plugin.reorder`.
- Returning to the config file's value: `DELETE /api/v1/plugins/order` (the screen's "Return to config file values"). It deletes the list saved in the DB and reloads the plugins in `[plugins].enabled` in that order (ones enabled only in Admin Studio become disabled). The audit log is `plugin.reset`. If a plugin in `[plugins].enabled` cannot be loaded, it is `400` and nothing changes.
- All responses have the same shape as `GET /api/v1/plugins/order`. With `[runtime].enable_wasm_plugins = false` it is `409`, and also `409` if a notification channel (`plugin:<name>` in `[notifications].channels`) would be dropped.

### 7.3 Reflection Across Multiple Servers

Changes in Admin Studio are announced to all servers over the same backend's event bus as the cache.

- `[cache].backend = "redis"`: announced with Redis Pub/Sub (channel `aegis:events`), and the receiving server rereads the DB's list (settings values are in the cache) and swaps. Usually they line up in under a second. Announcements missed while the connection to Redis was down are recovered by rereading everything when reconnected.
- `[cache].backend = "memory"`: for single-server setups only (sessions are also only inside the process). Starting a second server on the same DB stops at startup ([Configuration reference 2.4](./configuration.md#24-cache--cache--sessions)).
- Covered: toggling, reordering, returning to the config file's values, and plugin settings values (`PUT /api/v1/plugins/{name}/settings`).
- If a newly enabled plugin cannot be loaded on that server (for example, it is not installed only on that server), an error log is written and it keeps running with the current plugins. Place the same plugins on all servers.
- With `[runtime].enable_wasm_plugins = false`, no plugins are loaded regardless of `enabled`.
- Templates and samples are in `sdk/templates/*` and `sdk/examples/*` and are not included in `plugins/` (by default no plugins are enabled).

### 7.4 Installing from a ZIP Package and Signatures

From Admin Studio's plugin screen (drag and drop) or `POST /api/v1/plugins/install` (`admin`, the multipart `file` field), you can upload a ZIP and install it. Themes use the same mechanism with `POST /api/v1/themes/install` ([Theme guide](./theme_and_publishing_guide.md#installing-from-a-zip)).

1. The ZIP is extracted to a temporary directory. Paths with `..` and absolute paths (ZipSlip) are rejected, as are files over 50MB each, 100MB total, or 1,000 entries.
2. `plugin.manifest.json` is looked for at the ZIP's root or in a directory one level down.
3. The manifest and Wasm are validated, and it checks that `min_aegis_version` (semver) is at or below the current version.
4. The signature is checked (below).
5. It is placed in `plugins/<name>/`. **A plugin of the same name is replaced.** It is not enabled (enable it in [7.1](#71-toggling-on-and-off-from-admin-studio)). The audit log is `plugin.install`.

If validation fails, it is `400` and nothing is placed.

**Signature (`signature.json`)**: placed at the root of the package.

```json
{ "algorithm": "ed25519", "public_key": "<32-byte hex>", "signature": "<64-byte hex>", "target": "package" }
```

- `target` is only `"package"` (the default; omittable), and the signature is over a digest that lines up the paths and SHA-256 of all files other than `signature.json` (other values such as `"manifest"` are not accepted). Tampering with any file in the package, including Wasm and templates, is detected.
- With `[plugins].require_signatures = true`, packages without a signature are rejected. In that case you must set at least one public key in `[plugins].trusted_public_keys` (if empty, the server does not start and package validation is also refused). Even with `false`, if `signature.json` exists it is checked (rejected if invalid).
- `[plugins].trusted_public_keys` lists trusted Ed25519 public keys (32-byte hex) (when `require_signatures = false` and empty, a key is accepted if the signature is valid with any key). For themes it is the setting of the same name in `[theme]`.
