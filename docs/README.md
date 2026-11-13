# AegisCMS Documentation Portal (Documentation Index)

**English** | [日本語](README.ja.md)

> [!NOTE]
> The documents listed below are currently written in Japanese only. This index is provided in English and Japanese.

The official collection of documents on AegisCMS's design, configuration, feature specifications, extension development (themes and plugins), and operations.  
Refer to the most suitable document for what you want to know and for your role (site operator, theme author, plugin developer, core developer).

---

## 🗺️ Documentation Map

### 1. 🏗️ Core Architecture & Design
| Document | Overview | Audience |
|---|---|---|
| [Architecture design (`architecture.md`)](architecture.md) | Detailed design of the whole system: the crate layout, DB abstraction, publishing pipeline, cache/sessions, Wasm runtime, backups, and more | Core developers, operators |
| [Configuration file reference (`configuration.md`)](configuration.md) | A reference for every item in `config/default.toml`, environment variable overrides, and dynamic system settings from Admin Studio (`system.*`) | Operators, core developers |
| [Development guide (`development.md`)](development.md) | `task` commands, build steps, CI, local DB verification, E2E tests, Storybook, CLI reference, development tips | Core developers |
| [REST API specification (`openapi.json`)](openapi.json) | The REST API specification file, compliant with OpenAPI 3.0 | API integrators, developers |

### 2. 🔐 Security, Authentication & Auditing
| Document | Overview | Audience |
|---|---|---|
| [Authentication & user management (`authentication.md`)](authentication.md) | Local authentication (Argon2id), session management, OpenID Connect, SAML 2.0, account registration and invitation flows, API tokens (for LLMs and CI) | Operators, core developers |
| [Audit log reference (`audit.md`)](audit.md) | Destinations of operation audit logs (DB/log/Prometheus), the list of event types, retention period, viewing in Admin Studio | Operators, auditors |
| [Security assessment report (`security-assessment-report.md`)](security-assessment-report.md) | Internal inspection, OWASP ZAP scan results, and the status of vulnerability countermeasures | Security staff |
| [Penetration testing procedure (`penetration-testing.md`)](penetration-testing.md) | Building a safe local verification environment with Docker Compose, and vulnerability scanning steps | Security staff |
| [Benchmark comparison with WordPress (`benchmark.md`)](benchmark.md) | Results of running AegisCMS and WordPress under the same conditions in Docker (with and without cache), and how to reproduce them | Operations staff, evaluators |

### 3. 🎨 Theme Development & Frontend
| Document | Overview | Audience |
|---|---|---|
| [Theme authoring guide (`theme_authoring_guide.md`)](theme_authoring_guide.md) | A theme's directory layout, `theme.json`, template variables, CSS design, and how to use `theme dev` and Theme Builder | Theme authors |
| [Content publishing & theme engine development guide (`theme_and_publishing_guide.md`)](theme_and_publishing_guide.md) | The MiniJinja three-layer fallback mechanism, scheduled publishing/unpublishing, Atom feeds, and public routing design | Theme authors, developers |
| [Body blocks & Markdown reference (`blocks.md`)](blocks.md) | Headings, paragraphs, tables, TOC, callouts, custom HTML (sanitized / sandbox), and the plugin block specification | Content creators, authors |

### 4. 🔌 WebAssembly Plugin Extensions
| Document | Overview | Audience |
|---|---|---|
| [Official Wasm plugin development guide (`plugin_sdk_guide.md`)](plugin_sdk_guide.md) | Plugin development with the Rust / AssemblyScript / TinyGo SDKs, building, testing, and distribution and signing steps | Plugin developers |
| [Creating your first block plugin (`plugin_block_tutorial.md`)](plugin_block_tutorial.md) | Steps from a template to a plugin with a custom block that can be placed in the body, shown in an article on a real server (15 minutes) | Plugin developers |
| [Plugin dev server & Plugin Builder (`plugin_dev_server.md`)](plugin_dev_server.md) | Plugin development and debugging with the lightweight dev server (`aegis-server plugin dev`) and the real-time verification UI (Plugin Builder) | Plugin developers |
| [Versioning and releases (`versioning.md`)](versioning.md) | Versioning policy for the core, SDKs, and plugins, plugin compatibility (`min_aegis_version` / `abi_version`), and the release procedure with release-please | Developers, release managers |
| [Plugin hooks & host API reference (`plugin_hooks.md`)](plugin_hooks.md) | The specification of every available hook, data structures, manifest definition, host API, and dynamic execution order control | Plugin developers |
| [Common ABI specification (`plugin_abi_specification.md`)](plugin_abi_specification.md) | The low-level ABI signatures between a Wasm module and the host, memory layout, and data exchange conventions | SDK developers |

### 5. 🧩 Built-in Features & Services
| Document | Overview | Audience |
|---|---|---|
| [Media & storage management (`media.md`)](media.md) | Upload handling, image optimization, file size limits, storage operation API | Operators, developers |
| [Site search (`search.md`)](search.md) | Full-text search with tantivy, index building and automatic updates, search API / screen display | Operators, developers |
| [Member accounts (`members.md`)](members.md) | Visitor accounts (separate from admin accounts), shared by the shop (order history, reviews) and the forum. The API, screens for purchasers, password reset and email verification, withdrawal, the admin screen, settings, and migration from customer accounts | Operators, theme authors, developers |
| [Content posting API (`content-api.md`)](content-api.md) | A REST API guide for automatically posting, saving as draft, updating, and managing articles from external tools, CI/CD, and scripts | Developers, operators |
| [Headless delivery API (`headless.md`)](headless.md) | A read-only API that returns only published content as JSON without authentication. Usage, schema-driven responses, crate layout, safety, and caching | Core developers |
| [Analytics (`analytics.md`)](analytics.md) | Lightweight privacy-conscious analytics that uses no cookies, statistics collection, and display in Admin Studio | Operators |
| [Contact forms (`contact.md`)](contact.md) | Dynamic form generation from the config file, bot protection, notification integration, and the Admin Studio inbox | Operators, authors |
| [Cookie consent banner (`consent.md`)](consent.md) | A banner that keeps ads, analytics, and embeds that require consent from running until consent is given, how to write `data-aegis-consent`, and the JavaScript API | Operators, authors |
| [CSP policy (`csp.md`)](csp.md) | The current state of Content-Security-Policy for the public site and Admin Studio, and the design for a Strict CSP on public pages (hash approach, structured injection by plugins, `[security].strict_csp`) | Developers, operators |
| [Notice banners (`notices.md`)](notices.md) | Announcements shown at the top of all pages for a set period (maintenance, sales), the close button, how to write `notices.html`, and the API | Operators, authors |
| [Newsletter (`newsletter.md`)](newsletter.md) | Email address sign-up (double opt-in), unsubscribe links, bulk delivery of published articles, and management in Admin Studio | Operators, authors |
| [Redirects (`redirects.md`)](redirects.md) | 301 / 302 redirects from old URLs to new URLs, automatic rules when a slug changes, the API, and management in Admin Studio | Operators, authors |
| [Tags (`tags.md`)](tags.md) | Managed tags that can be attached to any content, the API, management and assignment in Admin Studio, and backups | Operators, authors, developers |
| [Comment management (`comments.md`)](comments.md) | Accepting comments on articles and products, moderation (approval workflow, banned words, IP limits), and notifications | Operators, authors |
| [Forum system (`forum.md`)](forum.md) | A threaded forum, separate DB setup, keyset pagination, reporting and moderation | Operators, authors |
| [E-commerce (`commerce.md`)](commerce.md) | Products and variations, cart, inventory reservation, order status transitions, and payment integration | Operators, developers |
| [Declarative queries (`queries.md`)](queries.md) | Aggregations and conditional lists declared in YAML (counts, sums, top N). The host runs them and passes only the results to themes and plugins. The `[queries]` settings, secondary tables (public-side columns), and time zones | Authors, developers |
| [Notification service (`notifications.md`)](notifications.md) | Event notifications by email (SMTP) and webhooks, localized templates, and queued delivery | Operators, developers |

---

## 🎯 Recommended Reading Order by Role

### 👩‍💼 Site Operators / Administrators
1. [Configuration file reference (`configuration.md`)](configuration.md) - basic settings and dynamic settings from the admin screen (`system.*`)
2. [Authentication & user management (`authentication.md`)](authentication.md) - user registration, role permissions, SSO integration
3. [Audit log (`audit.md`)](audit.md) / [Analytics (`analytics.md`)](analytics.md) - operational monitoring and access statistics
4. [Content posting API (`content-api.md`)](content-api.md) - automatic posting and update flows from external tools and scripts

### 🎨 Web Designers / Theme Authors
1. [Theme authoring guide (`theme_authoring_guide.md`)](theme_authoring_guide.md) - theme structure and fast development with `theme dev` / Theme Builder
2. [Body blocks & Markdown reference (`blocks.md`)](blocks.md) - elements inserted into article bodies and CSS styling
3. [Content publishing & theme engine development guide (`theme_and_publishing_guide.md`)](theme_and_publishing_guide.md) - public URL routing and detailed MiniJinja specifications

### 🧩 Plugin Extension Developers
1. [Official Wasm plugin development guide (`plugin_sdk_guide.md`)](plugin_sdk_guide.md) - getting started from a template, and debugging
2. [Plugin dev server & Plugin Builder (`plugin_dev_server.md`)](plugin_dev_server.md) - instant local verification and interactive testing
3. [Plugin hooks & host API reference (`plugin_hooks.md`)](plugin_hooks.md) - the complete specification of hooks and the host API
4. [Common ABI specification (`plugin_abi_specification.md`)](plugin_abi_specification.md) - the low-level memory ABI (for example, when building an SDK for a new language)

### 🛠️ AegisCMS Core Developers / Contributors
1. [Agent operating conventions (`AGENTS.md`)](../AGENTS.md) - branch conventions, commit conventions, PR verification rules
2. [Development guide (`development.md`)](development.md) - `task` commands, running tests, local verification environment
3. [Architecture design (`architecture.md`)](architecture.md) - the architecture and internal structure of the whole system
4. [Task management (`TODO.md`)](../TODO.md) - current status and outstanding tasks
