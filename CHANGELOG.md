# Changelog

## [0.4.0](https://github.com/takumi-mr/AegisCMS/compare/v0.3.0...v0.4.0) (2026-11-05)


### ⚠ BREAKING CHANGES

* **server:** [forum].allow_anonymous is replaced by [forum].posting, a category's is_private is replaced by visibility (public / members / staff), and threads and posts have author_kind. Old databases need the SQL in docs/members.md; old backups are restored with the same meaning.
* **core:** [commerce.customers] is now [members] (default disabled, with its own public_url), /api/v1/customers/* is /api/v1/members/*, _aegis_customers is _aegis_members, customer_id is member_id, the session token prefix and the cookie change (everyone is signed out), and audit actions and notification templates are member.*. Old backups can still be restored. See docs/members.md.
* **db:** a database created by v0.3.x or older is no longer upgraded when the server starts. The system tables (users, orders, forum, publications) get every column from CREATE TABLE; the ALTER TABLE blocks that added columns to old tables, and the back-fill of the publication columns, are removed (about 540 lines, three times over for SQLite, PostgreSQL and MySQL). init() now checks that the columns an older version lacked are there (SELECT ... LIMIT 0) and, if not, stops with a message that names the way out: start v0.3.x once so it upgrades the database, `task dump`, then `task restore` with this version. Content tables get the same check for unpublish_at.
* **server:** POST/PUT /api/v1/contents/{type} take the schema's custom fields under `fields` (`{"title": "...", "fields": {"excerpt": "..."}}`) instead of at the top level, so a write and a read have the same shape. A top-level key that is not a standard field is refused with 400 (`unknown field`) instead of being dropped silently, so a client still using the old form fails loudly.
* **db:** the _aegis_migrations table is no longer created or written. It was never read; it only recorded when each schema was last applied. An existing table is left in place and can be dropped.
* **server:** plugin HTTP requests to ports other than 80/443/8080 or to internal IP addresses are now refused, and redirects are followed (with the allowed domains checked on every hop) instead of being returned as 3xx.

### Features

* add i18n support to plugin-builder and theme-builder ([#255](https://github.com/takumi-mr/AegisCMS/issues/255)) ([1728d59](https://github.com/takumi-mr/AegisCMS/commit/1728d59c45b0126f77c125e854a8b3111d00f008))
* **admin:** add open source licenses section to settings and root LICENSE ([#251](https://github.com/takumi-mr/AegisCMS/issues/251)) ([dce416a](https://github.com/takumi-mr/AegisCMS/commit/dce416a63969798b638854eb826fea4c1810ff41))
* **admin:** add the Customers screen to Admin Studio ([#249](https://github.com/takumi-mr/AegisCMS/issues/249)) ([b6eede6](https://github.com/takumi-mr/AegisCMS/commit/b6eede6635d8bfd71b188f4e7499370109c6bb18))
* **admin:** add the Reviews screen to Admin Studio ([#250](https://github.com/takumi-mr/AegisCMS/issues/250)) ([dac7155](https://github.com/takumi-mr/AegisCMS/commit/dac7155558649be4a764bb045a741a1d3552520c))
* **admin:** edit schema fields without a dedicated input in the editors ([#229](https://github.com/takumi-mr/AegisCMS/issues/229)) ([ccd4e63](https://github.com/takumi-mr/AegisCMS/commit/ccd4e63b01e11274cb10093e492e35040a1bc0f4))
* **admin:** show forum activity on the member detail and a member badge in forum moderation ([#258](https://github.com/takumi-mr/AegisCMS/issues/258)) ([611affa](https://github.com/takumi-mr/AegisCMS/commit/611affabc87de9d3f8e467a8b338e73acb56d9bd))
* **admin:** show the Members tab without the shop and add member and category screenshots ([#260](https://github.com/takumi-mr/AegisCMS/issues/260)) ([8f7f6d5](https://github.com/takumi-mr/AegisCMS/commit/8f7f6d53d61733f43794709307c10d968d0601a3))
* **ast:** render inline images (badges) and turn URL-only paragraphs into link cards ([#225](https://github.com/takumi-mr/AegisCMS/issues/225)) ([4680c43](https://github.com/takumi-mr/AegisCMS/commit/4680c433a29a2af2a0b99ce2d354c07b10dce91c))
* **commerce:** add customer password reset and end sessions on suspend and password change ([#246](https://github.com/takumi-mr/AegisCMS/issues/246)) ([79e4257](https://github.com/takumi-mr/AegisCMS/commit/79e4257b13cae81f263c3175dcc74f99536ffcf3))
* **commerce:** add email address confirmation for customers ([#247](https://github.com/takumi-mr/AegisCMS/issues/247)) ([d75fc18](https://github.com/takumi-mr/AegisCMS/commit/d75fc188122f055766578e57234495858aac4059))
* **commerce:** add the [commerce.customers] settings ([#243](https://github.com/takumi-mr/AegisCMS/issues/243)) ([f3a7544](https://github.com/takumi-mr/AegisCMS/commit/f3a75443c03f21a8a4632001f9c50a0eea243e02))
* **commerce:** add the customer account pages and cookie sessions ([#244](https://github.com/takumi-mr/AegisCMS/issues/244)) ([4a1b650](https://github.com/takumi-mr/AegisCMS/commit/4a1b650dd0de52c8c5460f19aabbf5c38616b7fd))
* **commerce:** add the review section to the product page ([#245](https://github.com/takumi-mr/AegisCMS/issues/245)) ([1ccb5b1](https://github.com/takumi-mr/AegisCMS/commit/1ccb5b1929425c13d8c1132be79d9e291b3b23b6))
* **commerce:** let a customer delete their account ([#248](https://github.com/takumi-mr/AegisCMS/issues/248)) ([7c1bf31](https://github.com/takumi-mr/AegisCMS/commit/7c1bf31617b5c8b4bfc68db6d68295bf2182a723))
* **db:** resolve relations from published records only ([#235](https://github.com/takumi-mr/AegisCMS/issues/235)) ([13fca37](https://github.com/takumi-mr/AegisCMS/commit/13fca37d3070c2b03a1c7d005d33aed6c43c66e8))
* **headless:** add the headless delivery crate ([#236](https://github.com/takumi-mr/AegisCMS/issues/236)) ([363295e](https://github.com/takumi-mr/AegisCMS/commit/363295e3a1de69d8bf199e6eab43027bd137cda0))
* **headless:** publish an OpenAPI for the delivery API and finish its docs ([#238](https://github.com/takumi-mr/AegisCMS/issues/238)) ([863b84c](https://github.com/takumi-mr/AegisCMS/commit/863b84c416958687f33afba214750b4f84e176a8))
* **members:** add a public name (nickname) to the member account ([#256](https://github.com/takumi-mr/AegisCMS/issues/256)) ([eb5543b](https://github.com/takumi-mr/AegisCMS/commit/eb5543b5814d4d53716e453d5c9702d907e35b76))
* **schema:** add a public flag to schema fields ([#234](https://github.com/takumi-mr/AegisCMS/issues/234)) ([6016e7e](https://github.com/takumi-mr/AegisCMS/commit/6016e7e26749ad69d18ceb62cb4e8fd8144af3da))
* **schema:** make category_id a built-in field on every content type ([#228](https://github.com/takumi-mr/AegisCMS/issues/228)) ([e0be161](https://github.com/takumi-mr/AegisCMS/commit/e0be16139021e06ffab3c5bcb2c17648573304e9))
* **server:** let members post in the forum and limit categories to members or staff ([#257](https://github.com/takumi-mr/AegisCMS/issues/257)) ([be66613](https://github.com/takumi-mr/AegisCMS/commit/be66613111cb1552589639e53a47018120872701))
* **server:** oEmbed provider, stricter plugin network policy and official-link-embed ([#226](https://github.com/takumi-mr/AegisCMS/issues/226)) ([a907365](https://github.com/takumi-mr/AegisCMS/commit/a9073653879b7f484d7036c6c6055521ce8addd4))
* **server:** review changes to published content while the live version stays ([#227](https://github.com/takumi-mr/AegisCMS/issues/227)) ([51a0321](https://github.com/takumi-mr/AegisCMS/commit/51a03210db340bfcb422885069b2067aae9e20c0))
* **server:** serve the headless delivery API ([#237](https://github.com/takumi-mr/AegisCMS/issues/237)) ([48634f5](https://github.com/takumi-mr/AegisCMS/commit/48634f5fb8aea895412eda87932d1208da57742b))


### Bug Fixes

* **admin:** wait for a product's extra fields before saving, and test the category_id migration ([#232](https://github.com/takumi-mr/AegisCMS/issues/232)) ([15ebb14](https://github.com/takumi-mr/AegisCMS/commit/15ebb1499b732d7eff516a8f7a8d3495990bbce5))
* **server:** run the tests that share a real database one at a time ([#230](https://github.com/takumi-mr/AegisCMS/issues/230)) ([e5021e6](https://github.com/takumi-mr/AegisCMS/commit/e5021e6f3410f5e4424f69e6a3816164dc77e0e8))


### Performance

* **db:** filter published lists by tag in SQL and drop the legacy slow path ([#222](https://github.com/takumi-mr/AegisCMS/issues/222)) ([e28ed60](https://github.com/takumi-mr/AegisCMS/commit/e28ed60bb166e4e0b1044ec80b6907e92920df74))


### Refactoring

* **core:** move the customer accounts out of [commerce] and call them members ([#253](https://github.com/takumi-mr/AegisCMS/issues/253)) ([713bb25](https://github.com/takumi-mr/AegisCMS/commit/713bb250b5b86605a365d460946d4cd1f20eed8c))
* **core:** move the member accounts into the aegis-members crate ([#254](https://github.com/takumi-mr/AegisCMS/issues/254)) ([cca5640](https://github.com/takumi-mr/AegisCMS/commit/cca56401f89ac4d9cb0d99ed412b0ac29a5e71d1))
* **db:** remove the unused migrations table and duplicate helpers ([#239](https://github.com/takumi-mr/AegisCMS/issues/239)) ([f460abb](https://github.com/takumi-mr/AegisCMS/commit/f460abb31b8f87bb0c22f6a7137b3552e23859f1))
* **db:** stop upgrading old system tables in place ([#241](https://github.com/takumi-mr/AegisCMS/issues/241)) ([7cb730a](https://github.com/takumi-mr/AegisCMS/commit/7cb730a6698e7cc4db917c9339cbdfaa910db672))
* **server:** write custom fields under `fields`, like the responses ([#240](https://github.com/takumi-mr/AegisCMS/issues/240)) ([8a6c333](https://github.com/takumi-mr/AegisCMS/commit/8a6c333c91880da66ba517c8fe2113eb6763b3cf))

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
