# AegisCMS セキュリティ監査レポート（レースコンディション重点調査）

## 1. 調査概要

AegisCMS（v0.13.0）のバックエンド全主要クレート（認証・OAuth/SSO・セットアップ・コマース・決済・サブスクリプション・会員・キャッシュ・ストレージ・Wasm ランタイム・DB・フォーラム・コメント・メルマガ・通知キュー・AST/レンダリング・Headless API・oEmbed・Analytics）を対象に、ソースコードおよび SQLite / PostgreSQL / MySQL マイグレーション定義の静的コード監査を実施しました。

特に **レースコンディション（TOCTOU、非不可分な Check-Then-Act、Lost Update、単一利用トークンの同時リプレイ、ロック境界の不備）** を重点的に検証し、併せて調査過程で確認されたロジックバグ起因のセキュリティ脆弱性も整理しています。

> [!IMPORTANT]
> 本レポートの指摘は、ソースコードと DB マイグレーション定義を直接読んで確かめたものです。既存の [security-assessment-report.ja.md](security-assessment-report.ja.md)（OWASP ZAP による動的スキャン）では見つけにくい、並行処理とビジネスロジックの層を中心に調べました。
>
> 初版のあと、全 29 項目をコードと突き合わせて再検証し、**深刻度と優先度を振り直しました**（第 2 版）。事実と違う指摘はありませんでしたが、攻撃の前提条件（必要な権限・成立するタイミング）を考えると深刻度が高すぎた項目、修正方針が足りなかった項目、新たに見つかった問題がありました。振り直しの理由は [再トリアージ](#再トリアージ第-2-版) にまとめています。

### 検出件数サマリー（第 2 版）

| 区分                                            | Critical | High | Medium | Low |   合計 |
| ----------------------------------------------- | -------: | ---: | -----: | --: | -----: |
| **Part A: レースコンディション / 並行処理起因** |        0 |    3 |     10 |   5 | **18** |
| **Part B: ロジックバグ起因のその他脆弱性**      |        0 |    5 |      4 |   2 | **11** |

初版の表（Part A: Critical 1 / High 7 / Medium 9 / Low 3、合計 20）は本文の見出し（18 項目、High は 8 項目）と合っていなかったため、本文に合わせて数え直しています。

### 深刻度と優先度の基準

- **深刻度**: 影響の大きさに加えて、攻撃に必要な前提（未認証で届くか・正規の権限やトークンが要るか・admin しか操作できないか）と、成立のしやすさ（決定的に起きるか・数 ms の競合が要るか）で決めます。
- **優先度**:
    - **P0（即時）**: 前提が軽く影響が大きいもの、または修正がごく小さく効果が大きいもの。
    - **P1（次スプリント）**: 影響は大きいが前提が重いもの、または修正に設計変更・マイグレーションが要るもの。
    - **P2（堅牢化）**: 運用事故に近いもの、admin しか起こせないもの、成立する時間が非常に狭いもの。

---

## 再トリアージ（第 2 版）

| ID   | 初版        | 第 2 版 | 優先度 | 振り直しの理由                                                                                                                                                                                                          |
| ---- | ----------- | ------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A-01 | Critical    | Medium  | P1     | 未初期化の `/setup` に届く攻撃者は、競合させなくても先に申し込めば admin を取れる。競合で増える被害は「正規の管理者と同時に申し込んだときの admin の追加」だけ                                                          |
| A-02 | High        | Medium  | P1     | 攻撃できるのは正規の招待トークンを持つ本人だけで、得られる権限は招待ロールと同じ。増えるのは「1 つ無効化されても別アカウントが残る」居座り                                                                              |
| A-03 | High        | High    | P0     | 変更なし。無効化・降格したアカウントが有効なセッションを持ち続ける。リクエストごとに `disabled` を再確認する処理がないため、失効が唯一の防御                                                                            |
| A-04 | High        | Medium  | P1     | パスワードとバックアップコードの両方が既に必要。ただし Fail-Open（DB 更新失敗でコードが使い続けられる）は修正が小さいので P0 の作業に含める                                                                             |
| A-05 | High        | Low     | P2     | 管理者 2 人の同時操作による運用事故で、外部からの攻撃ではない                                                                                                                                                           |
| A-06 | High        | Medium  | P1     | 会員がプロフィール更新を処理中の瞬間に停止・降格が重なる必要がある。会員が更新を連打して狙うことはできる                                                                                                                |
| A-07 | High        | High    | P1     | 変更なし。`charge.refunded` の照合不一致・累計額の加算はレースではなく決定的に起きる。修正の順番に注意（本文参照）                                                                                                      |
| A-08 | High        | High    | P1     | 変更なし。Stripe は Webhook の到着順を保証しないため、解約済み契約の復活は現実に起こりうる                                                                                                                              |
| A-09 | High        | Medium  | P1     | 影響はレビュー評価の操作に限られる。slug と UUID による 2 件投稿は決定的に起きる                                                                                                                                        |
| A-10 | Medium-High | Medium  | P1     | 並列バーストで上限を数倍超えられるが、ロック自体は数本遅れで効く                                                                                                                                                        |
| A-11 | Medium      | Medium  | P0     | 競合の窓は数 ms と狭いが、本文に追記したリフレッシュトークン削除エラーの握りつぶしは決定的に失効を無効化しうる。修正が小さいため P0                                                                                     |
| A-12 | Medium      | Medium  | P1     | 変更なし                                                                                                                                                                                                                |
| A-13 | Medium      | Medium  | P1     | 変更なし                                                                                                                                                                                                                |
| A-14 | Medium      | Low     | P2     | 影響がロック済みスレッドへの割り込みとクールダウン回避（スパム）に限られる                                                                                                                                              |
| A-15 | Medium      | Medium  | P2     | 全購読者への二重送信は起こりうるが、操作できるのは管理者だけ。「確認用トークンと配信停止用トークンが同じ」は仕様上の設計で脆弱性ではないため取り下げ                                                                    |
| A-16 | Medium      | Low     | P2     | 成立する時間が非常に狭く、影響は通知の重複・リトライ間隔の無視                                                                                                                                                          |
| A-17 | Medium      | Low     | P2     | パッケージを導入できるのは admin だけ                                                                                                                                                                                   |
| A-18 | Low-Medium  | Low     | P2     | 変更なし。記事購入は `provider_ref` の UNIQUE で同一決済からの重複は防げている                                                                                                                                          |
| B-01 | Critical    | High    | P0     | セッションが既に乗っ取られていることが前提。ただし修正は 1 行なので最優先                                                                                                                                               |
| B-02 | High        | High    | P0     | 変更なし                                                                                                                                                                                                                |
| B-03 | High        | Medium  | P0     | パスワードを持つ攻撃者が DB 障害の瞬間を引く必要がある。修正は 1 行なので P0                                                                                                                                            |
| B-04 | High        | High    | P0     | 深刻度は変更なし。修正方針が足りなかったため書き直した（本文参照）                                                                                                                                                      |
| B-05 | High        | Medium  | P1     | `member_id` は注文に保存されるだけで、会員価格などの判定には使われない。被害は他会員の注文履歴の汚染と、その注文でのレビュー資格。修正は 1 行                                                                           |
| B-06 | High        | High    | P0     | タグ注入は確認済み。文字参照による `javascript:` のすり抜けは実際に動かして確かめるまで未確定（P2 に分離）                                                                                                              |
| B-07 | High        | High    | P0     | 変更なし                                                                                                                                                                                                                |
| B-08 | Medium      | Low     | P2     | `require_self_or_admin` は完全一致で比べるため、悪用できるのは admin だけ                                                                                                                                               |
| B-09 | Medium      | Medium  | P1     | 変更なし                                                                                                                                                                                                                |
| B-10 | Medium      | Medium  | P2     | 5 点の影響がそろっていない。2 の oEmbed は、仕様（[members.ja.md 15.4](members.ja.md#154-漏れる面すべて同じ判定を通す)）どおり一覧・フィードと同じ公開範囲の要約しか返さない誤検知のため取り下げ（本文参照）。残りは P2 |
| B-11 | Low         | Low     | P2     | 変更なし                                                                                                                                                                                                                |

---

## Part A: レースコンディション / 並行処理に起因する脆弱性・不具合

### A-01. [Medium / P1] 初期セットアップ（`/api/v1/setup`）の TOCTOU レースにより複数の管理者アカウント作成が可能

- **該当箇所**:
    - [handlers/setup.rs](../crates/aegis-server/src/handlers/setup.rs#L63-L141)
    - [setup.rs](../crates/aegis-server/src/setup.rs#L85-L159)
- **発生メカニズム**:
    1. `POST /api/v1/setup` が `is_initialized()`（`list_users().is_empty()`）を確認した後、`run_init()` 内で新規 DB 接続作成・スキーマ読み込み・マイグレーション実行・再度の `is_initialized()` 確認・Argon2id ハッシュ化（数百 ms）を経て `_aegis_users` に `role = "admin"` を挿入します。
    2. プロセス内ミューテックス、DB トランザクション、および「初期化は 1 回限り」を強制する DB 側の一意制約（シングルトン行）のいずれも存在しません（`_aegis_users` の一意制約は `username` のみ）。
- **影響**: 未初期化状態で異なる `admin_username` を指定した `POST /api/v1/setup` が同時到着すると、双方が初期化チェックを通過し、複数の `admin` アカウントが未認証で作成されます。
- **深刻度の補足**: 未初期化の `/setup` に届く攻撃者は、競合させなくても先に申し込むだけで admin を取れます。競合で増える被害は「正規の管理者と同時に申し込んだときに admin がもう 1 つできる」ことだけです。本当のリスクは未初期化のまま公開されている時間の長さであり、運用手順（初期化前は外部に公開しない）と併せて対処します。
- **修正方針**:
    - ユーザー作成前に、主キー制約を利用した不可分な初期化クレーム（例: `_aegis_settings` への `INSERT` で `system.initialized` を先行取得し、重複時は `403 ALREADY_INITIALIZED`）または排他トランザクション + ロックを導入する。
    - `run_init` で毎回 DB 再接続・マイグレーション再実行を行わず、既存の `state.db` を使用する。

---

### A-02. [Medium / P1] 招待トークンの同時リプレイ（非不可分 `get` + `delete` と `email` 一意制約欠如）による無制限の特権アカウント作成

- **該当箇所**:
    - [invitation.rs](../crates/aegis-auth/src/invitation.rs#L212-L277)
    - [0001_baseline.sql (SQLite)](../crates/aegis-db/migrations/sqlite/0001_baseline.sql#L42-L57)（PostgreSQL / MySQL も同様）
- **発生メカニズム**:
    1. `accept_invitation` は `cache.get(&key)` の後に別呼び出しで `cache.delete(&key)` を実行しており、OIDC / SAML / OAuth コード等で採用されている `cache.increment("{key}:used") == 1` による不可分ガードがありません。
    2. 続くメール重複チェックは非不可分な `db.list_users()` 走査であり、その後に重い Argon2id ハッシュ化（`spawn_blocking`）が挟まるため、競合ウィンドウが非常に広くなります。
    3. 3 方言すべての `_aegis_users` テーブルで `email` 列に `UNIQUE` 制約がありません。
- **影響**: 1 つの招待トークンに対して異なる `username` で同時リクエストを送ることで、招待ロール（最大 `admin`）を持つ有効アカウントを複数作成できます。
- **深刻度の補足**: 攻撃できるのは正規の招待トークンを持つ招待された本人だけで、得られる権限は招待ロールと同じです。増える被害は「アカウントを 1 つ無効化されても別のアカウントが残る」居座りです。
- **修正方針**:
    - `cache.increment(&format!("{key}:used"), Some(ttl)) == 1`（または `CacheBackend::take`）をトークン読取直後に導入する。
    - `_aegis_users` の正規化済み `email` に DB レベルの `UNIQUE` インデックスを追加する。

---

### A-03. [High / P0] 2FA 待機中チャレンジおよびログイン処理中（Argon2 実行中）の無効化・権限降格・セッション失効すり抜け

- **該当箇所**:
    - [handlers/auth.rs](../crates/aegis-server/src/handlers/auth.rs#L149-L180) / [L262-L390](../crates/aegis-server/src/handlers/auth.rs#L262-L390)
    - [local.rs](../crates/aegis-auth/src/local.rs#L47-L83)
    - [member.rs](../crates/aegis-members/src/member.rs#L604-L655)
- **発生メカニズム**:
    1. **2FA チャレンジ（最大 300 秒ウィンドウ）**: `create_session` が当時の `identity`（`role` を含む）をキャッシュに保存し、`verify_2fa` は DB から `user` を再取得しているにもかかわらず **`user.disabled` を確認せず**、またキャッシュ内の古い `identity.role` をそのまま使って新規セッションを発行します。新規セッションの `created_at` は発行時刻となるため、チャレンジ中に実行された `revoke_user_sessions`（`revoked_at`）より新しい時刻となり失効を免れます。
    2. **パスワード検証中の競合（スタッフ・会員共通）**: `LocalAuthProvider::authenticate` および `MemberService::login` は Argon2id 検証の**前**にレコードを取得し、検証後に古いスナップショットの `disabled`/`status` のみを見て現在時刻でセッションを作成します。検証中にアカウント無効化・停止・削除・パスワード変更が行われても、`issued_at > revoked_at` の有効なセッションが発行されます。
- **影響**: 無効化・停止・降格・削除されたアカウントが、新規の有効セッション（降格前の `admin` 権限を含む）を最大セッション有効期間（会員は最大 30 日）保持し続けられます。リクエストごとに `disabled` を DB で再確認する処理はなく、セッション失効が唯一の防御です。
- **修正方針**:
    - `verify_2fa` で DB の最新 `user.disabled`・`user.role` を再検証・再反映する。
    - ログイン処理の開始時刻（DB 読取前）をセッションの `created_at` / `issued_at_micros` の基準とする、あるいは発行直前に DB の `status`/`disabled`/`password_hash` を再検証する。失効の判定は [session.rs:100](../crates/aegis-auth/src/session.rs#L100) で既に `created_at <= revoked_at` になっているため、比較の向きは変えなくてよい。問題は `created_at` に発行時刻が入ることである（より堅くするなら DB の単調増加 `session_version` へ移行する）。

---

### A-04. [Medium / P1（Fail-Open の修正は P0）] 2FA バックアップコードの同時再利用レースおよび DB 更新失敗時の Fail-Open

- **該当箇所**:
    - [handlers/auth.rs](../crates/aegis-server/src/handlers/auth.rs#L288-L340)
    - [users.rs](../crates/aegis-auth/src/users.rs#L170-L179)
- **発生メカニズム**:
    1. パスワード認証を複数回通して別々の `challenge_token` を取得し、同一バックアップコードで `verify_2fa` を同時送信すると、両方が同じ `user.backup_codes` を読み込み、メモリ上で検証後に無条件 `UPDATE _aegis_users ... WHERE id = ?` で上書きするため、1 回限りのバックアップコードが複数回通ります。
    2. さらに [handlers/auth.rs](../crates/aegis-server/src/handlers/auth.rs#L323-L334) では `update_backup_codes` が `Err` を返してもログ出力のみで `is_valid = true` としてセッションを発行してしまいます（Fail-Open）。
- **影響**: 単一利用であるべきバックアップコードから複数セッションの発行が可能になります。Fail-Open の場合は、そのコードが消費されずに使い続けられます。
- **深刻度の補足**: パスワードとバックアップコードの両方を既に持っていることが前提です。
- **修正方針**:
    - バックアップコードハッシュごとに `cache.increment("backup_code_used:{username}:{hash}") == 1` を必須化するか、SQL の楽観ロック（`WHERE id = ? AND backup_codes = ?` で `rows_affected == 1` を検証）を用いる。
    - DB 更新失敗時は `500` で拒否（Fail-Closed）する。

---

### A-05. [Low / P2] 最後のアクティブ管理者保護の TOCTOU レース（全管理者ロックアウト）

- **該当箇所**:
    - [users.rs](../crates/aegis-auth/src/users.rs#L204-L220), [L249-L258](../crates/aegis-auth/src/users.rs#L249-L258), [L276-L283](../crates/aegis-auth/src/users.rs#L276-L283)
- **発生メカニズム**:
  `ensure_other_admin` が `db.list_users()` で他のアクティブ管理者数を数えた後、トランザクションもロックもなく別クエリで `update_user` / `delete_user` を実行します。`update_user` では `ensure_other_admin` の直後に Argon2id の `hash_password`（L253）が走るため、競合ウィンドウがさらに広がります。
- **影響**: 残り 2 名の管理者を同時に降格・無効化・削除すると双方が `others == 1` と判定して成功し、有効な管理者が 0 名になり Admin Studio 全体が管理不能になります。
- **深刻度の補足**: 起こせるのは admin だけで、管理者 2 人の同時操作による運用事故です（CLI からの復旧手段があれば影響はさらに小さい）。
- **修正方針**: パスワードハッシュ化は事前に済ませた上で、管理者数の確認と更新/削除を排他トランザクション（SQLite は `BEGIN IMMEDIATE`、PostgreSQL/MySQL は `SELECT ... FOR UPDATE`）内で不可分に実行する。

---

### A-06. [Medium / P1] `_aegis_members` 全列上書き（Lost Update）レースによる自己停止解除および会員ランク維持

- **該当箇所**:
    - [aegis-db/src/members.rs](../crates/aegis-db/src/members.rs#L194-L225)
    - [aegis-members/src/member.rs](../crates/aegis-members/src/member.rs#L784-L846)（`update_profile`）, [L1037-L1057](../crates/aegis-members/src/member.rs#L1037-L1057)（`complete_password_reset`）
    - [handlers/members.rs](../crates/aegis-server/src/handlers/members.rs#L447-L473)（`update_me`）
- **発生メカニズム**:
    1. `db.update_member` は `WHERE id = ?` で `status`・`level`・`password_hash` を含む全 11 列を無条件に上書きします。
    2. `update_profile` はレコード取得後、パスワード変更時に **2 回の Argon2id 処理**（現行検証 + 新規ハッシュ化）を挟んでから `update_member` を実行します。
    3. この間に管理者が会員を停止（`status = 'suspended'`）またはランク降格（`level = 'free'`）しても、`update_profile` が古い `status = 'active'` / `level = 'vip'` で書き戻します。直後に `update_me` が `reissue_session` を呼ぶため、最新の `revoked_at` より新しい有効セッションまで再発行されます。
- **影響**: 停止処分やランク剥奪を受けた会員が、プロフィール更新・パスワード変更の競合により自己復帰・権限維持できてしまいます。
- **深刻度の補足**: 停止・降格の瞬間に会員の更新処理が重なる必要があります。会員が更新を連打して狙うことはできるため Medium とします。
- **修正方針**: 全列 `update_member` を廃止し、変更対象列だけを更新する SQL（プロフィール列のみ、`password_hash` のみ、`status` のみ、`level` のみ等）に分離し、会員自身による更新には `AND status = 'active'` 条件を付与する。

---

### A-07. [High / P1] 注文返金・在庫戻しの非不可分レース（在庫水増し・過剰返金）と `charge.refunded` Webhook の不整合

- **該当箇所**:
    - [aegis-db/src/orders.rs](../crates/aegis-db/src/orders.rs#L312-L325)
    - [aegis-commerce/src/lib.rs](../crates/aegis-commerce/src/lib.rs#L826-L950), [L995-L1116](../crates/aegis-commerce/src/lib.rs#L995-L1116)
    - [aegis-commerce/src/stripe.rs](../crates/aegis-commerce/src/stripe.rs#L137-L138), [L413-L439](../crates/aegis-commerce/src/stripe.rs#L413-L439)
- **発生メカニズム**:
    1. **楽観ロックの欠如**: `build_order_refund_update` は `WHERE id = ?` のみで `refunded_amount` や `status` の前提条件をチェックしていません。`restock: true` の返金リクエストが同時実行されると、双方が「未戻し」と判定して在庫を 2 回加算（在庫水増し）し、`refunded_amount` / `refunds_json` も後勝ちで破損します。
    2. **キャンセルとの競合**: Stripe 返金 HTTP 呼び出し中（L873）に別リクエストで注文が `cancelled` へ遷移すると、`transition_order` と `record_order_refund` の双方で在庫が戻され、実在庫以上に増加します。
    3. **同額部分返金の Idempotency-Key 衝突**: [stripe.rs:138](../crates/aegis-commerce/src/stripe.rs#L138) が `aegis-refund-{order.id}-{amount}` を使用しているため、同額の部分返金を 2 回行うと 2 回目は Stripe 側で実返金されずキャッシュ応答が返り、DB 上だけ返金額が二重加算されます。
    4. **`charge.refunded` Webhook の照合不一致と二重加算**: `stripe.rs:416` は `payment_intent` (`pi_...`) を `payment_ref` に詰めますが、注文側の `payment_ref` には Checkout Session ID (`cs_...`) が入っており（`pi_...` は `transaction_id`）、照合が失敗して外部返金が黙殺されます。さらに `charge["amount_refunded"]` は累計額であるにもかかわらず `order.refunded_amount + amount` で加算する構造になっています。
- **影響**: 存在しない在庫の過剰販売（在庫水増し）、返金会計の破損、および Stripe 管理画面からの返金未反映。
- **修正方針**:
    - `record_order_refund` に `WHERE id = ? AND refunded_amount = ? AND status = ?` の楽観ロック条件を追加し、`cancelled` 状態での在庫戻しを禁止する。
    - Stripe の返金 `Idempotency-Key` に返金連番または試行 UUID を含める。
    - `charge.refunded` は `transaction_id` で照合し、累計 `amount_refunded` との差分または個別 refund オブジェクト ID で冪等に反映する。
- **修正の順番に関する注意**: 現状は `payment_ref` の照合が失敗するため、アプリ自身が Stripe API で行った返金の `charge.refunded` も無視されており、そのおかげで二重加算がたまたま起きていません。照合だけを先に直すと、アプリ自身の返金が二重に加算されます。照合の修正は、`refund_ref`（Stripe の refund ID）による重複排除と、累計額との差分による反映と同時に入れること。あわせて、`refunds.data[0]` だけを見ている点（複数回の返金で最新以外を取りこぼす）も直す。

---

### A-08. [High / P1] サブスクリプション Webhook の順序逆転・競合による解約済み契約の復活（有料コンテンツ不正閲覧）

- **該当箇所**:
    - [handlers/subscription.rs](../crates/aegis-server/src/handlers/subscription.rs#L231-L271), [L546-L710](../crates/aegis-server/src/handlers/subscription.rs#L546-L710)
    - [aegis-db/src/subscriptions.rs](../crates/aegis-db/src/subscriptions.rs#L196-L227)
- **発生メカニズム**:
    1. `update_subscription` は状態遷移チェックや Stripe イベント時刻（`event.created`）の単調性チェックなしに `WHERE id = ?` で無条件上書きします。`sync_subscription` も Stripe API から最新状態を再取得せず、到着した Webhook ペイロードをそのまま書き込みます。
    2. 管理者が即時解約（`cancel_now`）した直後や `customer.subscription.deleted` の後に、遅延・再送された `customer.subscription.updated`（`status: "active"`）が到着すると、DB の状態が `cancelled` から `active` に書き戻され、期末 + 3 日間まで有料・VIP コンテンツの閲覧権が復活します。
    3. さらに `start_subscription`（L231-271）は既存の有効契約チェック後にランダム UUID の Idempotency-Key で Checkout Session を作成するため、同時実行で同一会員に複数の課金セッション・重複契約が作成されます。
- **影響**: 解約・強制停止後の有料コンテンツ不正閲覧、および会員への二重課金。
- **修正方針**:
    - `sync_subscription` 内で Stripe API から最新の契約状態を再取得して反映する、または `cancelled` からの復帰を SQL 条件（`WHERE id = ? AND status != 'cancelled'`）と `event.created` の単調増加チェックで禁止する。
    - `start_subscription` の重複防止に会員単位の排他または DB 制約を導入する。

---

### A-09. [Medium / P1] 商品レビューの同時重複投稿レース（DB `UNIQUE` 制約なし）および ID / Slug 二重投稿バイパス

- **該当箇所**:
    - [aegis-commerce/src/review.rs](../crates/aegis-commerce/src/review.rs#L212-L215), [L258-L321](../crates/aegis-commerce/src/review.rs#L258-L321)
    - [0001_baseline.sql (SQLite)](../crates/aegis-db/migrations/sqlite/0001_baseline.sql#L100-L104)（PostgreSQL / MySQL も同様）
- **発生メカニズム**:
    1. `submit_review` は `get_review_by_order_product` で事前 `SELECT` し、さらに設定読取クエリ（L271）を挟んでから `insert_review` を実行します。3 方言すべてのマイグレーションで `idx_aegis_reviews_order_product` は通常の `INDEX` であり **`UNIQUE` 制約ではありません**。
    2. さらに非レースの確定的バイパスとして、L214 は `l.product_id == product_id || l.slug == product_id` で照合するにもかかわらず `product_id` を正規化せずパスパラメータのまま保存するため、1 注文で UUID と slug の両方から確実に 2 件投稿できます。
- **影響**: 1 回の購入で無制限の「購入確認済み」レビューを投稿でき、商品評価や SEO 構造化データ（`AggregateRating`）を操作できます。
- **修正方針**: 一致した `OrderLine.product_id` へ正規化し、新規マイグレーションで `_aegis_reviews (order_id, product_id)` に `UNIQUE` 制約を追加して重複エラーを `400` へマップする。

---

### A-10. [Medium / P1] ログイン・2FA・コンテンツパスワードのブルートフォース制限の同時バースト回避（TOCTOU）

- **該当箇所**:
    - [aegis-auth/src/rate_limit.rs](../crates/aegis-auth/src/rate_limit.rs#L86-L158)
    - [handlers/auth.rs](../crates/aegis-server/src/handlers/auth.rs#L45-L128), [L276-L346](../crates/aegis-server/src/handlers/auth.rs#L276-L346)
    - [handlers/members.rs](../crates/aegis-server/src/handlers/members.rs#L358-L390) / [handlers/access.rs](../crates/aegis-server/src/handlers/access.rs#L584-L696)
- **発生メカニズム**:
  `LoginRateLimiter::check()` はロックキーの有無を読むだけで、失敗回数の加算・ロック設定（`record_failure_with_max_ip`）は重い Argon2id 検証が完了した**後**に実行されます。そのため、N 本の並列ログイン試行を同時送信すると、最初の 1 本がロックキーを書く前に全 N 本が `check()` を通過してパスワード検証を実行します（`verify_2fa` でも事前に複数の `challenge_token` を取得して並列送信することで同様に回避可能）。
- **影響**: ユーザー単位（既定 5 回）・IP 単位（既定 20 回）のロックアウト上限を並列バーストで大幅に超過して試行でき、Argon2id の CPU 負荷増幅にもつながります。
- **修正方針**: 認証検証の**前**に `cache.increment` で試行枠を先行確保（またはアカウント/IP ごとの同時実行数制限を導入）し、認証成功時のみクリア/減算する。

---

### A-11. [Medium / P0] API トークン失効と OAuth リフレッシュトークン更新の TOCTOU レース、および削除エラーの握りつぶし

- **該当箇所**:
    - [handlers/api_tokens.rs](../crates/aegis-server/src/handlers/api_tokens.rs#L187-L199)
    - [oauth.rs](../crates/aegis-server/src/oauth.rs#L1371-L1502)
- **発生メカニズム**:
    1. `revoke_api_token` はアクセストークン削除（L189）とリフレッシュトークン削除（L198）を別クエリで実行します。その隙に `POST /oauth/token`（`refresh_token`）が `take_oauth_refresh_by_hash` を実行すると、紐づく `access_token_id` の残存確認を行わずに新しいアクセストークンと 90 日有効なリフレッシュトークンを発行します（競合の窓は数 ms）。
    2. **（第 2 版で追記・レースではない）** [api_tokens.rs:196-199](../crates/aegis-server/src/handlers/api_tokens.rs#L196-L199) はリフレッシュトークン削除の結果を `let _` で捨てています。この削除が失敗すると、アクセストークンの失効は `204` で成功扱いになる一方、リフレッシュトークンが残り続け、いつでも新しいアクセストークンを取り直せます。
- **影響**: 管理画面でトークン失効（`204 No Content`）した直後でも、OAuth クライアントが新しい永続アクセス権を獲得できます。
- **修正方針**: リフレッシュトークンを先に削除してからアクセストークンを削除し、削除エラーは `500` で返す（Fail-Closed）。
- **対応時の判断**: 「リフレッシュ時に `access_token_id` のアクセストークンが残っていることを確かめる」案は採用しない。`issue_tokens` は期限切れの OAuth アクセストークンを片付けるがリフレッシュトークンは残すため、この確認を入れると、アクセストークンの期限切れ後の正規のリフレッシュまで失敗する。削除順の入れ替えにより、失効が完了した後に届いたリフレッシュは必ず失敗し、残る競合は「失効より前に始まったリフレッシュ」（失効の前に起きた操作として扱える）だけになる。

---

### A-12. [Medium / P1] パスワード再設定・新規登録・メール確認トークンの非不可分消費（`get` + `delete`）および重複メール登録レース

- **該当箇所**:
    - [password_reset.rs](../crates/aegis-auth/src/password_reset.rs#L154-L182)
    - [registration.rs](../crates/aegis-auth/src/registration.rs#L192-L225)
    - [member.rs](../crates/aegis-members/src/member.rs#L560-L565), [L1029-L1035](../crates/aegis-members/src/member.rs#L1029-L1035)
- **発生メカニズム**:
    1. 4 箇所すべてが `cache.get(&key)` → `cache.delete(&key)` の非不可分な 2 ステップで単一利用トークンを消費しており、並列リクエスト（特に Redis 構成）で同一トークンが複数回読み取られます。
    2. `RegistrationService::complete` は `_aegis_users.email` に `UNIQUE` 制約がない状態で `list_users()` による非不可分重複確認を行うため、同一メールの別ユーザー名アカウントが同時登録で重複作成されます。
    3. `PasswordResetService::complete`（L163-181）は `user.disabled` を確認しておらず、再設定申請後に無効化されたユーザーでもパスワードを変更できます。
- **影響**: 単一利用トークンの同時リプレイ、同一メールアドレスでの複数スタッフアカウント作成（パスワード再設定対象の不整合）、無効化アカウントのパスワード書き換え。
- **修正方針**: `CacheBackend::take`（メモリは単一 WriteLock での `remove`、Redis は `GETDEL` または Lua スクリプト）または `cache.increment("{key}:used") == 1` を全トークン消費に統一導入し、`PasswordResetService::complete` で `user.disabled` を拒否する。

---

### A-13. [Medium / P1] 会員退会（`delete_me`）中の競合により解約漏れサブスクリプションと匿名化漏れ PII が残留

- **該当箇所**:
    - [handlers/members.rs](../crates/aegis-server/src/handlers/members.rs#L600-L674)
    - [handlers/subscription.rs](../crates/aegis-server/src/handlers/subscription.rs#L512-L581)
- **発生メカニズム**:
  `delete_me` は Stripe 解約 HTTP 通信や注文・レビュー・フォーラム投稿の匿名化をすべて終えた**最後**（L655）に会員行の削除と `revoke_sessions` を実行します。この長い I/O ウィンドウの間に、①並行到着した `checkout.session.completed` が新規サブスクリプションを挿入して継続課金が残る、②まだ有効な会員セッションで新規レビュー・注文・フォーラム投稿を作成でき匿名化されずに PII が残る、③アバター差し替えにより孤立オブジェクトが残る、という問題が起きます。
- **影響**: 退会済み会員への継続課金、退会後の PII 消去・匿名化漏れ。
- **修正方針**: パスワード確認直後に会員ステータスを `deleting` / `suspended` へ更新して `revoke_sessions` を先行実行し、`start_from_checkout` でも `member.status == "active"` を検証してからクリーンアップと最終削除を行う。
- **対応時の判断**: 状態は `deleting` にした（`suspended` と区別して、管理者の操作で戻せないようにする）。後始末に失敗したら `active` に戻す（会員がもう一度ログインして、やり直せるように）。`start_from_checkout` は、会員がいない・`active` でないときに Stripe で解約する（以前は会員がいないとき、記録を残すだけで課金が続いていた）。契約を保存したあとにも会員の状態を見直す（退会の側は `deleting` にしてから契約を解約するので、どちらが先でも、どちらかが相手を見る）。

---

### A-14. [Low / P2] フォーラムのスレッドロック・自動ロック上限（`auto_lock_post_count`）およびクールダウン制限の TOCTOU レース

- **該当箇所**:
    - [aegis-forum/src/lib.rs](../crates/aegis-forum/src/lib.rs#L327-L365), [L766-L821](../crates/aegis-forum/src/lib.rs#L766-L821), [L980-L1058](../crates/aegis-forum/src/lib.rs#L980-L1058)
    - [aegis-db/src/forum.rs](../crates/aegis-db/src/forum.rs#L815-L895)
- **発生メカニズム**:
    1. **ロック回避**: `create_post` は事前に `thread.is_locked` を確認しますが、`db.insert_post` 内の `UPDATE _aegis_forum_threads SET post_count = post_count + 1 ... WHERE id = ?` に `AND is_locked = 0` がなく、自動ロックも `insert_post` コミット後の別クエリで実行されます。
    2. **クールダウン回避**: `check_rate_limit`（`cache.get`）と `record_rate_limit`（`cache.set`）がモデレーション・DB 挿入を挟んで分離しており、同時送信された投稿がすべてクールダウン（`rate_limit_secs`）をすり抜けます。
- **影響**: ロック済み・上限到達済みスレッドへの割り込み投稿、および `rate_limit_secs` の完全回避によるスパム連投。
- **修正方針**: `insert_post` の `UPDATE` に `AND is_locked = 0 AND status = 'approved'`（および自動ロック条件）を組み込み、クールダウンは投稿処理開始前に `cache.increment(&key, Some(rate_limit_secs)) == 1` で不可分に確保する。

---

### A-15. [Medium / P2] メルマガ配信（`send_issue`）の二重送信 TOCTOU レース・ロック期限切れ・購読者状態競合

- **該当箇所**:
    - [aegis-newsletter/src/lib.rs](../crates/aegis-newsletter/src/lib.rs#L367-L516), [L648-L756](../crates/aegis-newsletter/src/lib.rs#L648-L756)
    - [aegis-db/src/newsletter.rs](../crates/aegis-db/src/newsletter.rs#L176-L189)
- **発生メカニズム**:
    1. **二重送信**: `send_issue` はロック取得（L669）より**前**に `latest_issue()`（L658）を確認し、ロック取得後に再確認しません。また `SEND_LOCK_SECS = 300` 秒を超える大量配信ではロックが自然失効し、先発ワーカー終了時の無条件 `cache.delete(&lock)`（L683）が後発ワーカーのロックまで消してしまいます。
    2. **配信停止の復活・メール連打**: `build_subscriber_update` が状態・トークン条件なしの `WHERE id = ?` のみのため、`confirm` と `unsubscribe` が競合すると退会済み購読者が `active` に戻り、`subscribe` の並列送信で 60 秒スロットルを突破して確認メールを連打できます。（初版にあった「`confirm` 時に確認用トークンと配信停止トークンが同一のまま」という指摘は、トークンが宛先本人にしか届かない仕様上の設計であり脆弱性とは言いにくいため、第 2 版で取り下げました。）
- **影響**: 全購読者へのメルマガ二重送信、オプトアウト済みユーザーの意図しない再購読、確認メールフラッド。
- **修正方針**: ロック取得後に `latest_issue` を再確認（または DB に `sending` 状態の issue 行を先行作成）、ロック解放をオーナートークン一致時のみとし、購読者更新に `AND status = ?` 条件を追加する。

---

### A-16. [Low / P2] 通知キューワーカーの Claim レース（バックオフ回避・試行回数上書き・リース期限切れ競合）

- **該当箇所**:
    - [aegis-db/src/notifications.rs](../crates/aegis-db/src/notifications.rs#L147-L172)
    - [aegis-notify/src/lib.rs](../crates/aegis-notify/src/lib.rs#L395-L562)
    - [handlers/notifications.rs](../crates/aegis-server/src/handlers/notifications.rs#L320-L338)
- **発生メカニズム**:
    1. `build_notification_claim` に **`AND next_attempt_at <= ?`** が欠落しており、Claim 後に最新行を再読込もしません。ワーカー A・B が同じジョブを取得し、B が即失敗して `attempts = 1, next_attempt_at = now + 2s, locked_until = NULL` で解放した直後に A が Claim すると、バックオフを無視して即再実行し、古い `attempts = 0` を元に `attempts = 1` で上書きします。
    2. `release_notification` / `delete_notification` が `WHERE id = ?` のみで自身のリース所有権（`locked_until` やリーストークン）を検証しないため、300 秒超の遅延配信時に別ワーカーと衝突・二重送信・状態破壊が起こります。
    3. 管理者の削除ハンドラ（`delete_queued_notification`）は期限切れ `locked_until` でも `is_some()` で拒否し、かつ `get` → `delete` が非不可分です。
- **影響**: リトライ間隔の無視・失敗上限の遅延、クラスタ運用時のメール/Webhook 重複送信、クラッシュしたワーカーのジョブ削除不能。
- **深刻度の補足**: 1 の競合は「B が取得・失敗・解放するまでの間に A が送信権を取る」という非常に狭い時間でしか成立しません。
- **修正方針**: Claim SQL に `AND next_attempt_at <= ?` を追加し、リリース/完了削除にはリース一致条件（`AND locked_until = ?`）と SQL 側の `attempts = attempts + 1` を使用する。

---

### A-17. [Low / P2] プラグイン・テーマのパッケージインストール時の非不可分ディレクトリ差し替えレース

- **該当箇所**:
    - [handlers/plugin.rs](../crates/aegis-server/src/handlers/plugin.rs#L653-L668)
    - [handlers/theme.rs](../crates/aegis-server/src/handlers/theme.rs#L192-L207)
- **発生メカニズム**:
  `install_plugin_package` および `install_theme_package` は `plugin_registry.lock()` やファイルロックを取らずに `remove_dir_all(&target_dir)` → `copy_dir_all(&pkg_root, &target_dir)` を実行しています。
- **影響**: 同一プラグイン/テーマの同時更新や、`set_plugin_enabled` / `sync_from_db` / アセット配信（`get_plugin_asset`・`get_active_theme_asset`）と競合すると、ディレクトリ消失による 500 エラーや、異なるバージョンの `signature.json` / `plugin.manifest.json` / `plugin.wasm` が混在した不整合状態が発生し得ます。
- **（第 2 版で追記・レースではない）**: `remove_dir_all` の後で `copy_dir_all` が失敗すると、元のプラグイン/テーマが消えたまま戻りません（ロールバックがない）。アトミックリネームによる配置で同時に解消します。
- **深刻度の補足**: パッケージを導入できるのは admin だけです。
- **修正方針**: `plugin_registry.lock()`（テーマは専用ロック）で直列化し、同一ファイルシステム上の一時ディレクトリへ展開後にアトミックリネーム（あるいはバックアップ差し替え）で配置する。

---

### A-18. [Low / P2] 記事購入（`_aegis_purchases`）重複作成・コメント重複/承認通知・バージョン採番の軽微な競合

- **該当箇所**:
    - **記事購入重複**: [handlers/purchase.rs](../crates/aegis-server/src/handlers/purchase.rs#L205-L267), [L713-L744](../crates/aegis-server/src/handlers/purchase.rs#L713-L744) — `(member_id, content_type, content_id)` の paid 一意制約がなく、同時決済後に 1 件だけ取消/返金しても別行により閲覧権が残存する。なお `provider_ref` は UNIQUE のため、同一の決済セッションからの重複登録は防げており、残る問題は別々の決済セッションを作られた場合のみ。
    - **コメント重複・承認通知重複**: [aegis-comments/src/lib.rs](../crates/aegis-comments/src/lib.rs#L517-L597), [L868-L923](../crates/aegis-comments/src/lib.rs#L868-L923) — `has_duplicate_comment` と `insert_comment` が非不可分、また複数モデレーターの同時承認で事前スナップショットに基づき重複メールが送信される。
    - **コンテンツ版番号採番衝突**: [sqlite.rs](../crates/aegis-db/src/sqlite.rs#L614-L620) / [postgres.rs](../crates/aegis-db/src/postgres.rs#L629-L635) / [mysql.rs](../crates/aegis-db/src/mysql.rs#L624-L630) — `insert_content_version` がトランザクション外で `MAX(revision) + 1` を計算するため、同時更新で主キー衝突が発生し得る。
    - **その他軽微なレース**: [api_tokens.rs](../crates/aegis-auth/src/api_tokens.rs#L126-L148)（20 件上限の同時超過）、[oauth.rs](../crates/aegis-server/src/oauth.rs#L95-L118)（初回起動時の署名鍵生成競合）、[orders.rs](../crates/aegis-db/src/orders.rs#L430-L450)（カート商品順による DB 行ロック順序の不一致デッドロック）。

---

## Part B: 調査中に併せて発見したロジックバグ起因の脆弱性

レースコンディションではありませんが、深刻度が高く直ちに対処すべき実装バグが確認されました。

### B-01. [High / P0] 2FA 無効化エンドポイントで任意の非空パスワードが通る（`Result<bool>::is_ok()` 誤用）

- **該当箇所**: [handlers/users.rs](../crates/aegis-server/src/handlers/users.rs#L719-L724), [password.rs](../crates/aegis-auth/src/password.rs#L45-L51)
- **詳細**:
  `verify_password(&pass, &user.password_hash)` はパスワード不一致時に **`Ok(false)`** を返しますが、`disable_2fa` が `.is_ok()` で判定しているため、任意の 1 文字（例: `"x"`）を送るだけで常に `verified = true` になります。
- **影響**: 乗っ取ったログインセッション（パスワードも TOTP 端末も持たない状態、あるいはローカルパスワードを持たない SSO ユーザー）から `POST /api/v1/users/me/2fa/disable` で 2FA を無効化できます。
- **深刻度の補足**: セッションが既に乗っ取られていることが前提のため High とします。修正が 1 行で効果が大きいため、優先度は P0 です。
- **修正方針**: `matches!(verify_password(...), Ok(true))` に修正し、`spawn_blocking` 内で実行する。

---

### B-02. [High / P0] `POST /api/v1/users/me/2fa/enable` の再認証・既存 2FA 確認欠如による 2FA 乗っ取り

- **該当箇所**: [handlers/users.rs](../crates/aegis-server/src/handlers/users.rs#L666-L702)
- **詳細**: 既に `totp_enabled == true` かどうかを確認せず、現在のパスワード・既存 TOTP コードの再確認もなしに、クライアントが送った任意の `secret` で `totp_secret` と `backup_codes` を上書きします。
- **修正方針**: 既に 2FA が有効なら拒否し、有効化時に現在パスワード確認とサーバー生成シークレット（セッション紐付け）の検証を必須化する。

---

### B-03. [Medium / P0] ログイン時の DB エラーによる 2FA の Fail-Open バイパス

- **該当箇所**: [handlers/auth.rs](../crates/aegis-server/src/handlers/auth.rs#L143-L148)
- **詳細**: `state.db.get_user_by_username(&username).await.ok().flatten()` により、DB 一時障害やロックタイムアウト時の `Err` が `None` として握りつぶされ、2FA をスキップしてそのままセッションを発行します。
- **深刻度の補足**: パスワードを持つ攻撃者が、DB 障害の瞬間を引く必要があります。修正は 1 行のため P0 とします。
- **修正方針**: `Err` 時は `500` で認証を中断（Fail-Closed）する。

---

### B-04. [High / P0] 会員の事前登録（`unverified`）パスワード上書きによるアカウント乗っ取り

- **該当箇所**: [aegis-members/src/member.rs](../crates/aegis-members/src/member.rs#L417-L474), [L547-L594](../crates/aegis-members/src/member.rs#L547-L594)
- **詳細**: `require_email_verification = true` の環境で、被害者が登録し確認メールをクリックする前（最大 48 時間）に、第三者が同じメールアドレスと攻撃者パスワードで再登録すると、同一 `member_id` の `password_hash` が上書きされるにもかかわらず**以前の確認トークンが無効化されません**。被害者が正規メールのリンクをクリックすると、攻撃者パスワードでアカウントが有効化されます（さらに 3 通/時のメール制限到達後は通知メールすら飛ばずに上書き可能）。
- **修正方針（第 2 版で書き直し）**:
    - 初版の「旧トークンを無効化する」だけでは塞がりません。攻撃者が再登録した時点で被害者のアドレスに新しい確認メールが届き、被害者がそちらをクリックしても攻撃者のパスワードで有効化されるためです。
    - 根本対策として、**未確認アカウントの `password_hash` を再登録で上書きしない**。登録時のパスワードハッシュ（と氏名などの入力）は確認トークン側のデータ（キャッシュの `PendingToken`）に持たせ、確認リンクをクリックしたときに初めて会員の行へ反映する。これにより、各確認リンクは「そのリンクを発行した登録者のパスワード」でしか有効化できなくなる。
    - 反映時の更新は `WHERE id = ? AND status = 'unverified'` を条件とする。
- **対応時の判断（確認時のパスワード）**: 登録の内容をトークンに持たせるだけでは、攻撃者が被害者の直後に登録し直し、被害者が新しい方のメール（攻撃者の登録のリンク）を開くと、攻撃者のパスワードで有効になる。送り直しのリンク（メールアドレスだけで申し込める）も、攻撃者が先に登録していれば同じになる。そこで**確認のときに、そのリンクの登録のパスワードの入力を必須にした**（`POST /api/v1/members/verify` の `password`。違えば `401`、5 回でリンクを無効化）。送り直しのリンクは会員の行（最初の登録）のパスワードで有効になる。仕様は [members.md の 5](members.ja.md)。

---

### B-05. [Medium / P1] ショップ注文でのクライアント指定 `CartRequest.member_id` 信頼による他会員への注文なりすまし紐付け

- **該当箇所**: [aegis-commerce/src/lib.rs](../crates/aegis-commerce/src/lib.rs#L102-L117), [handlers/shop.rs](../crates/aegis-server/src/handlers/shop.rs#L208-L231)
- **詳細**: `POST /api/v1/shop/checkout` が `if request.member_id.is_none()` の場合のみログイン会員から補完するため、未認証リクエストが JSON に任意の `member_id`（レビューやフォーラムの `avatar_url` / `author_id` で公開されている UUID）を指定すると、そのまま他会員の注文履歴に紐付けられます。
- **深刻度の補足**: `member_id` は注文に保存されるだけで、会員価格・クーポンなどの判定には使われていません。被害は他会員の注文履歴が汚れることと、その注文によるレビュー投稿資格が付くことに限られます。
- **修正方針**: `CartRequest.member_id` に `#[serde(skip_deserializing)]` を付与し、必ずセッションからのみ設定する（修正は 1 行）。

---

### B-06. [High / P0（2 のエンティティ回避は要検証・P2）] 未検証 `BlockNode.name` タグ注入 + サンドボックス CSP ハッシュ記録連鎖による Stored XSS / Strict CSP 回避 & HTML サニタイザのエンティティ回避

- **該当箇所**:
    - [aegis-ast/src/renderer.rs](../crates/aegis-ast/src/renderer.rs#L21-L31), [L75-L137](../crates/aegis-ast/src/renderer.rs#L75-L137), [L179-L224](../crates/aegis-ast/src/renderer.rs#L179-L224), [L698-L708](../crates/aegis-ast/src/renderer.rs#L698-L708)
    - [aegis-ast/src/rewriter.rs](../crates/aegis-ast/src/rewriter.rs#L319-L346)
- **詳細**:
    1. `HtmlRenderer::render_node` のフォールバック分岐が `node.name` をバリデーションなしに `<{tag}>` として出力するため、`name: "script"` 等で任意の HTML タグを挿入できます。さらに `render_sandboxed_html`（L201-202）がユーザー入力 `raw` 内のスクリプトハッシュを親ページの Strict CSP に登録する仕様と組み合わさることで、Strict CSP 有効時でも同一オリジンでのスクリプト実行が可能になります。
    2. `sanitize_html_fragment_with_frames` は `lol_html` の未デコード属性値に対して `is_unsafe_uri` を適用するため、HTML 文字参照（数値実体参照等）で難読化した `javascript:` URI を検出できません。また `remove_scripts`（プラグイン出力用）も script 要素と `on*` のみ除去し `javascript:` URL・iframe `srcdoc`・meta 要素等が残ります。
    - **第 2 版の注記**: エンティティによるすり抜けは、lol_html の `Attribute::value()` が文字参照をデコードしないことが前提です。コードを読む限りありうる話ですが、実際に動かして確かめるまでは未確定とし、P2 に分けます。確かめる際は `&#106;avascript:` のような値を入れたテストを書く。
- **修正方針**: フォールバック出力のタグ名を厳格なカスタム要素規則（ハイフン必須の小文字英数字）に限定し、サニタイズはホワイトリスト方式（`ammonia` 等またはエンティティデコード後のスキーム許可リスト）へ移行する。
- **対応時の判断（CSP ハッシュ登録）**: `render_sandboxed_html` が `srcdoc` 内のスクリプトのハッシュを親ページの CSP に登録する仕組みは残す。`srcdoc` の文書は親の CSP を引き継ぐため、登録しないとサンドボックス内のスクリプトが動かない。ハッシュが許すのは「まったく同じ中身の」スクリプトだけで、タグ注入を塞いだ後は親ページにその script 要素を置く経路がない。この前提はコードのコメント（`render_sandboxed_html`）にも残した。

---

### B-07. [High / P0] フォーラムの非公開カテゴリ閲覧制限バイパス・未承認スレッド漏洩・HTML エラー応答のエスケープ漏れ

- **該当箇所**:
    - **非公開カテゴリ漏洩**: [handlers/forum.rs](../crates/aegis-server/src/handlers/forum.rs#L1246-L1284) — `GET /api/v1/forum/threads` が `resolve_viewer`・`check_category_visible` を呼ばず、`members`/`staff` 限定カテゴリのスレッド一覧を未認証で返却。
    - **未承認スレッド投稿漏洩・返信・非公開通報 IDOR**: [handlers/forum.rs](../crates/aegis-server/src/handlers/forum.rs#L1414-L1445), [aegis-forum/src/lib.rs](../crates/aegis-forum/src/lib.rs#L980-L1021), [L1168-L1297](../crates/aegis-forum/src/lib.rs#L1168-L1297)。
    - **反射型 HTML 注入 / XSS + 通報フォーム CSRF 欠如**: [handlers/forum.rs](../crates/aegis-server/src/handlers/forum.rs#L685-L698), [L799-L808](../crates/aegis-server/src/handlers/forum.rs#L799-L808), [L897-L961](../crates/aegis-server/src/handlers/forum.rs#L897-L961) — エラーメッセージ・`category_slug`・`thread_id` を `escape_html` せず `Html(...)` で返却（Strict CSP オフ時は反射型 XSS、オン時もフォーム注入が可能）。
- **修正方針**: `api_list_all_threads` / `api_list_posts` / `create_post` / `report` に可視性・承認状態チェックを追加し、全 HTML エラー応答で `escape_html` を適用、`handle_create_report_form` に Same-Origin チェックを追加する。

---

### B-08. [Low / P2] MySQL 環境における大文字・小文字違いのユーザー名によるセッション失効漏れ・自己変更ガード回避

- **該当箇所**: [handlers/users.rs](../crates/aegis-server/src/handlers/users.rs#L309-L316), [L398](../crates/aegis-server/src/handlers/users.rs#L398), [L427](../crates/aegis-server/src/handlers/users.rs#L427), [L453](../crates/aegis-server/src/handlers/users.rs#L453)
- **詳細**: `is_self` 判定と `revoke_user_sessions` に DB の正規 `user.username` ではなく URL パスそのままの `username` を渡しているため、MySQL（既定で大小文字非区別照合）では `PATCH /api/v1/users/ALICE` のように大文字小文字を変えると `is_self` ガード（自分のパスワード変更時の現行パスワード確認・自己削除禁止等）が外れ、`auth:revoked_user:alice` への失効も効きません。
- **深刻度の補足**: `require_self_or_admin` はパスのユーザー名を完全一致で比べるため、悪用できるのは admin だけです。ただし、admin が大文字小文字の違うパスで他のユーザーを無効化すると、相手のセッションが失効されずに残ります（リクエストごとの `disabled` 再確認がないため）。
- **修正方針**: DB から取得した正規の `current.username` をすべての比較・失効キーに使用する。

---

### B-09. [Medium / P1] OIDC / SAML ログインの CSRF・セッション固定（ブラウザ紐付け Cookie 欠如）

- **該当箇所**: [handlers/oidc.rs](../crates/aegis-server/src/handlers/oidc.rs#L16-L48), [handlers/saml.rs](../crates/aegis-server/src/handlers/saml.rs#L17-L48), [handlers/sso.rs](../crates/aegis-server/src/handlers/sso.rs#L83-L145)
- **詳細**: `oauth.rs` では `aegis_oauth_sso` Cookie により開始ブラウザを紐付けていますが、通常の Admin Studio 向け OIDC/SAML ログインにはブラウザ紐付け Cookie がなく、攻撃者の認証済みコールバック URL / SAMLResponse を被害者ブラウザに踏ませて攻撃者アカウントへログインさせる Login CSRF が成立します。
- **修正方針**: `oauth.rs` と同様に短命な `HttpOnly; SameSite=Lax` バインディング Cookie を発行・検証する。

---

### B-10. [Medium / P2] 公開エンドポイントからの情報漏洩・データ露出・CSV インジェクション

- **該当箇所**:
    1. **[Low〜Medium] 公開一覧での `access_password` ハッシュ露出**: [handlers/public.rs](../crates/aegis-server/src/handlers/public.rs#L645-L672) — `summarize_records` が `access_password`（Argon2id ハッシュ）および `public: false` フィールドを除外せず `PageRenderContext.items` に載せるため、全 `on-render-page` プラグインおよびテーマに渡る。訪問者に見えるのはテーマがその値を出力した場合のみ。
    2. **[取り下げ・誤検知] oEmbed の制限付きコンテンツメタデータ**: [handlers/oembed.rs](../crates/aegis-server/src/handlers/oembed.rs#L82-L162) — アクセス制限を確かめる処理はないが、返すのはタイトル・書き手の `excerpt`・アイキャッチ画像だけで本文は返さない。制限した記事のタイトルと書き手の `excerpt` は、フィード・一覧・関連記事でも誰にでも出す要約として扱う仕様（[members.ja.md 15.4](members.ja.md#154-漏れる面すべて同じ判定を通す) に「oEmbed は `excerpt` フィールドだけなので、そのままでよい」と明記）であり、漏洩ではない。初版と第 2 版の再トリアージ（High・P0）はこの仕様を見落としていた。
    3. **[Low] 下書き商品のバリアント漏洩・公開期限切れ商品の購入可能**: [handlers/shop.rs](../crates/aegis-server/src/handlers/shop.rs#L571-L598)（`status == "published"` 未確認）、[aegis-commerce/src/lib.rs](../crates/aegis-commerce/src/lib.rs#L393-L402)（`unpublish_at` 未確認）。期限切れ商品は、スケジューラーが `unpublish_at` を過ぎた商品をアーカイブするため、購入できるのは次にスケジューラーが動くまでの間だけ。
    4. **[Low〜Medium] Analytics CSV フォーミュラインジェクション & 無制限カーディナリティ DoS**: [aegis-analytics/src/lib.rs](../crates/aegis-analytics/src/lib.rs#L307-L360), [L655-L677](../crates/aegis-analytics/src/lib.rs#L655-L677) — CSV エクスポートが先頭 `= + - @` を無害化していないことは確認済み。`POST /api/v1/analytics/event` のレート制限と `pending` マップ上限の欠如は、実際に動かして確かめるまで未確定。
    5. **[Medium] Host ヘッダーインジェクション（ショップ注文メール・戻り URL）**: [handlers/shop.rs](../crates/aegis-server/src/handlers/shop.rs#L60-L94) — `[commerce].public_url` 未設定時に未信頼 `Host` ヘッダーをそのまま採用。正規のショップのメールサーバーから偽のリンク入りメールを任意の宛先へ送れるため、フィッシングに使える。

---

### B-11. [Low / P2] その他の防御的改善点

- **Tokio ワーカースレッド上での同期 Argon2id 実行**: [handlers/users.rs:721](../crates/aegis-server/src/handlers/users.rs#L721) および [aegis-members/src/member.rs](../crates/aegis-members/src/member.rs#L371)（登録・ログイン・プロフィール更新）で `spawn_blocking` を使わず同期実行しており、高並列時にイベントループを枯渇させ得る。
- **非定数時間トークンハッシュ比較**: [aegis-commerce/src/review.rs:244](../crates/aegis-commerce/src/review.rs#L244) が通常の `!=` でハッシュ比較している（`shop.rs` 同様の定数時間比較に統一推奨）。
- **`themed_response` のエラー文言未エスケープ**: [handlers/public.rs:949-961](../crates/aegis-server/src/handlers/public.rs#L949-L961) が内部エラーをエスケープせず HTML 出力している。

---

## Part C: 適切に保護されていることを確認した主要領域

監査の中で以下の領域はレースコンディションや攻撃に対して堅牢に実装されていることを確認しました。

- **在庫引き当て・注文期限切れ/キャンセルの在庫戻し**（[aegis-db/src/orders.rs](../crates/aegis-db/src/orders.rs#L252-L285), [L422-L458](../crates/aegis-db/src/orders.rs#L422-L458), [L599-L634](../crates/aegis-db/src/orders.rs#L599-L634)）: 注文作成時の在庫減算は単一トランザクション内の `WHERE id = ? AND (stock IS NULL OR stock >= ?)` で不可分に実行され、同時購入による在庫マイナス・過剰販売は防止されています。
- **キャッシュの不可分カウンタ**（[aegis-cache/src/memory.rs](../crates/aegis-cache/src/memory.rs#L113-L139), [redis.rs](../crates/aegis-cache/src/redis.rs#L117-L134)）: `MemoryCacheBackend` は単一 `RwLock` 書き込みロック、`RedisCacheBackend` は Lua スクリプトで `INCR` + 初回 `EXPIRE` を不可分に実行します。
- **OIDC / SAML / OAuth コード / OAuth リフレッシュトークン / TOTP コードのリプレイ防止**: これらは `cache.increment("{key}:used") == 1` や `DELETE ... RETURNING` / `rows_affected() > 0` による不可分消費が実装されています（SAML の XSW 防御・RSA 鍵長検証・OIDC のアルゴリズム検証も適切）。
- **SSRF DNS リバインディング対策**（[ssrf.rs](../crates/aegis-server/src/ssrf.rs#L68-L87)）: `PublicOnlyResolver` を `reqwest::dns::Resolve` に組み込み、接続時の名前解決結果に対して毎回パブリック IP 検証を行うため DNS リバインディング TOCTOU が防がれています。
- **SQL インジェクション耐性**（`aegis-db` 全般・`aegis-schema/src/query.rs`）: ユーザー入力値はすべてバインドパラメータ化され、動的識別子はスキーマ定義と `is_valid_identifier` で厳格に検証されています。
- **SVG 直接閲覧時の XSS 防止・ストレージパストラバーサル防止**（[app.rs:1296-1307](../crates/aegis-server/src/app.rs#L1296-L1307), [local.rs:22-43](../crates/aegis-storage/src/local.rs#L22-L43)）: SVG 応答には `CSP: default-src 'none'` が強制付与され、ストレージキーのトラバーサル検証も二重に行われています。
- **Stripe Webhook 署名検証・サーバー側価格計算**（[stripe.rs:375-406](../crates/aegis-commerce/src/stripe.rs#L375-L406)）: 定数時間 HMAC-SHA256 検証・300 秒タイムスタンプ許容・金額/通貨のサーバー側検証が適切です。
- **フォーラムの `post_number` 採番**（[aegis-db/src/forum.rs:825-858](../crates/aegis-db/src/forum.rs#L825-L858)）: トランザクション先頭で親スレッド行の `UPDATE` により排他ロックを取得してから採番しており、重複採番を防いでいます。

---

## 推奨される修正優先順位（第 2 版）

1. **P0（即時対応）**: 前提が軽く影響が大きいもの、または修正がごく小さく効果が大きいもの。
    - 認証・2FA: **B-01**（`disable_2fa` の `.is_ok()` 誤用）、**B-02**（2FA 再登録の再認証欠如）、**B-03**（ログイン時 2FA の Fail-Open）、**A-03**（2FA 待機・ログイン中の無効化/降格すり抜け）、**A-04 の Fail-Open 部分**、**A-11**（リフレッシュトークン削除エラーの握りつぶしと失効順序）。
    - 公開側の漏洩・XSS: **B-06 の 1**（AST タグ注入と CSP ハッシュ登録）、**B-07**（フォーラム非公開カテゴリ漏洩・HTML 未エスケープ）。
    - 会員: **B-04**（事前登録パスワード上書き乗っ取り。第 2 版の修正方針で対応）。
2. **P1（次スプリント）**: 影響は大きいが前提が重いもの、または設計変更・マイグレーションが要るもの。
    - コマース・決済: **A-07**（返金・在庫戻しレースと `charge.refunded`。修正の順番に注意）、**A-08**（サブスク Webhook 順序逆転による解約復活）、**A-09**（レビュー重複 + UNIQUE 制約）、**B-05**（`CartRequest.member_id`）。
    - 会員: **A-06**（`_aegis_members` 全列上書き）、**A-13**（退会中の競合）、A-03 の会員ログイン部分。
    - トークン・SSO・初期化: **A-12**（`CacheBackend::take` による一括解消）、**A-02**（招待トークン + `email` UNIQUE）、**A-04 のレース部分**、**A-10**（ブルートフォース同時バースト）、**B-09**（OIDC/SAML Login CSRF）、**A-01**（`/setup` TOCTOU）。
3. **P2（堅牢化）**: 運用事故に近いもの、admin しか起こせないもの、成立する時間が非常に狭いもの、未確定のもの。
    - **A-05**、**A-14**、**A-15**、**A-16**、**A-17**、**A-18**、**B-08**、**B-10 の 1・3・4・5**、**B-11**、**B-06 の 2**（実際に動かして確かめてから対応）。
