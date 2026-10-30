//! 組み込みテンプレートとカートのスクリプトの文言（`[theme].locale`）。
//!
//! 公開サイトの訪問者に見せる文言だけを持つ。API のエラーメッセージは対象外で、常に英語
//! （[アーキテクチャ](../../../docs/architecture.md#191-api-レスポンスのエラーメッセージ英語のみ)）。
//!
//! キーの形式:
//! - `名前`: ふつうの文言
//! - `名前.one` / `名前.other`: 数（`n`）で出し分ける文言。`n` が 1 なら `.one`、それ以外は `.other`
//! - `js.名前`: カートのスクリプト（`/shop/cart.js`）が使う文言。サーバーが `bundle` で渡す
//!
//! `{名前}` はパラメーターの差し込み位置。値は HTML エスケープされる。文言自体は信頼できる静的な文字列で、
//! `<strong>` のようなマークアップを含むものがある（テンプレートの `t()` は結果を安全な文字列として返す）。
//! 属性の値に使う文言には `"` と `<` を入れない。

/// 選べる言語
pub const LOCALES: &[&str] = &["en", "ja"];

/// `(キー, en, ja)`
const MESSAGES: &[(&str, &str, &str)] = &[
    // --- 共通 ---
    ("nav.home", "Home", "ホーム"),
    ("admin.studio", "Admin Studio", "管理画面"),
    (
        "footer.rights",
        "All rights reserved.",
        "無断転載を禁じます。",
    ),
    ("staff", "Staff", "スタッフ"),
    ("reply", "Reply", "返信"),
    ("pagination.label", "Pagination", "ページ送り"),
    ("pagination.previous", "Previous", "前へ"),
    ("pagination.next", "Next", "次へ"),
    ("pagination.newer", "Newer", "新しい記事"),
    ("pagination.older", "Older", "過去の記事"),
    ("pagination.newer_stories", "Newer stories", "新しい記事"),
    ("pagination.older_stories", "Older stories", "過去の記事"),
    ("pagination.page", "Page {page} / {total}", "{page} / {total} ページ"),
    // --- 記事一覧・記事・固定ページ ---
    (
        "index.badge",
        "Zero-Trust Content Engine",
        "ゼロトラスト・コンテンツエンジン",
    ),
    ("index.latest", "Latest Published Stories", "最新の記事"),
    ("index.posts.one", "{n} post", "{n} 件"),
    ("index.posts.other", "{n} posts", "{n} 件"),
    ("author.by", "By {name}", "{name}"),
    ("author.team", "Aegis Team", "Aegis チーム"),
    ("author.default", "Aegis Author", "Aegis 著者"),
    ("author.verified", "Verified Creator", "認証済みクリエイター"),
    ("read_story", "Read story", "続きを読む"),
    (
        "empty.title",
        "No Published Content Yet",
        "公開済みのコンテンツはまだありません",
    ),
    (
        "empty.hint",
        "Create and publish articles from Admin Studio to see them listed here.",
        "管理画面から記事を作成して公開すると、ここに表示されます。",
    ),
    ("empty.launch", "Launch Admin Studio", "管理画面を開く"),
    ("article.published_on", "Published on {date}", "{date} に公開"),
    ("article.back", "Back to all stories", "記事一覧に戻る"),
    ("article.scroll_top", "Scroll to top", "ページの先頭へ"),
    // --- カテゴリ・検索・404 ---
    ("tag.label", "Tag", "タグ"),
    (
        "tag.empty",
        "No published content has this tag yet.",
        "このタグが付いた公開済みのコンテンツはまだありません。",
    ),
    ("tag.feed", "Feed (Atom)", "フィード（Atom）"),
    ("tags.title", "Tags", "タグ"),
    (
        "tags.empty",
        "No tags are in use yet.",
        "使われているタグはまだありません。",
    ),
    ("tags.count.one", "{n} item", "{n} 件"),
    ("tags.count.other", "{n} items", "{n} 件"),
    ("tags.aria", "Tags", "タグ"),
    ("category.label", "Category", "カテゴリ"),
    (
        "category.empty",
        "No published articles in this category yet.",
        "このカテゴリには公開済みの記事がまだありません。",
    ),
    ("category.feed", "Feed (Atom)", "フィード（Atom）"),
    ("search.label", "Search", "検索"),
    ("search.sort", "Sort by", "並び順"),
    ("search.sort_relevance", "Most relevant", "一致の良い順"),
    ("search.sort_newest", "Newest first", "新しい順"),
    ("search.results.one", "{n} result", "{n} 件の結果"),
    ("search.results.other", "{n} results", "{n} 件の結果"),
    (
        "search.empty",
        "No pages matched your search.",
        "検索に一致するページはありませんでした。",
    ),
    ("notfound.title", "Page Not Found", "ページが見つかりません"),
    (
        "notfound.message",
        "The page or article you are looking for does not exist, has been archived, or is in draft status.",
        "お探しのページまたは記事は存在しないか、アーカイブ済み、または下書きの状態です。",
    ),
    ("notfound.home", "Return to Homepage", "トップページに戻る"),
    ("maintenance.title", "Maintenance", "メンテナンス中"),
    // --- コメント ---
    ("comments.count.one", "{n} comment", "{n} 件のコメント"),
    ("comments.count.other", "{n} comments", "{n} 件のコメント"),
    ("comments.none", "No comments yet.", "まだコメントはありません。"),
    ("comments.leave", "Leave a comment", "コメントを投稿する"),
    ("comments.replying_to", "Replying to", "返信先:"),
    ("comments.cancel", "Cancel", "キャンセル"),
    ("comments.name", "Name", "名前"),
    (
        "comments.email",
        "Email (not published)",
        "メールアドレス（公開されません）",
    ),
    ("comments.body", "Comment", "コメント"),
    (
        "comments.honeypot",
        "Leave this empty",
        "空のままにしてください",
    ),
    (
        "comments.notify",
        "Email me when someone replies or my comment is approved",
        "返信があったときや、コメントが承認されたときにメールで知らせる",
    ),
    ("comments.submit", "Post comment", "コメントを投稿"),
    ("comments.sending", "Sending…", "送信中…"),
    (
        "comments.failed",
        "Your comment could not be posted. Please try again.",
        "コメントを投稿できませんでした。もう一度お試しください。",
    ),
    (
        "comments.pending",
        "Thank you! Your comment will appear once it has been approved.",
        "ありがとうございます。コメントは承認されると表示されます。",
    ),
    ("comments.closed", "Comments are closed.", "コメントの受付は終了しました。"),
    // --- ショップ ---
    ("shop.label", "Shop", "ショップ"),
    ("shop.all_products", "All products", "すべての商品"),
    ("shop.cart", "Cart", "カート"),
    ("shop.no_products", "No products yet.", "商品はまだありません。"),
    ("shop.sold_out", "Sold out", "売り切れ"),
    ("shop.sku", "SKU: {sku}", "SKU: {sku}"),
    ("shop.quantity", "Quantity", "数量"),
    ("shop.add_to_cart", "Add to cart", "カートに入れる"),
    ("shop.buy_now", "Buy now", "今すぐ購入"),
    ("shop.only_left", "Only {n} left", "残り {n} 点"),
    (
        "shop.cart_loading",
        "Loading your cart…",
        "カートを読み込んでいます…",
    ),
    (
        "shop.cart_needs_js",
        "The cart needs JavaScript.",
        "カートを使うには JavaScript が必要です。",
    ),
    (
        "shop.order_thanks",
        "Thank you for your order",
        "ご注文ありがとうございます",
    ),
    (
        "shop.order_loading",
        "Loading your order…",
        "注文を読み込んでいます…",
    ),
    // --- 掲示板 ---
    ("forum.label", "Forum navigation", "フォーラムのナビゲーション"),
    ("forum.name", "Forum", "フォーラム"),
    ("forum.title", "Community Forum", "コミュニティフォーラム"),
    (
        "forum.tagline",
        "Join discussions, ask questions, and share knowledge with fellow members.",
        "ディスカッションに参加したり、質問したり、知識を共有したりしましょう。",
    ),
    ("forum.categories", "Categories", "カテゴリ"),
    (
        "forum.no_categories",
        "No categories available.",
        "利用できるカテゴリはありません。",
    ),
    (
        "forum.threads_count",
        "<strong>{n}</strong> threads",
        "<strong>{n}</strong> スレッド",
    ),
    (
        "forum.posts_count",
        "<strong>{n}</strong> posts",
        "<strong>{n}</strong> 件の投稿",
    ),
    ("forum.recent", "Recent Discussions", "最近のディスカッション"),
    ("forum.pin", "PIN", "固定"),
    ("forum.pinned", "PINNED", "固定"),
    ("forum.locked", "LOCKED", "ロック中"),
    (
        "forum.in_category",
        "in <strong>{name}</strong>",
        "<strong>{name}</strong> 内",
    ),
    ("forum.by", "by {name}", "投稿者: {name}"),
    ("forum.last_active", "Last active", "最終更新"),
    ("forum.last_post", "Last post", "最終投稿"),
    ("forum.total_replies", "Total replies", "返信の合計"),
    ("forum.new_thread", "+ New Thread", "+ 新しいスレッド"),
    (
        "forum.started_by",
        "Started by <strong>{name}</strong>",
        "<strong>{name}</strong> が開始",
    ),
    ("forum.posts.one", "{n} post", "{n} 件の投稿"),
    ("forum.posts.other", "{n} posts", "{n} 件の投稿"),
    ("forum.created", "Created", "作成日"),
    (
        "forum.thread_pending",
        "Your thread was submitted and is awaiting moderator approval.",
        "スレッドを送信しました。モデレーターの承認をお待ちください。",
    ),
    (
        "forum.reply_pending",
        "Your reply was submitted and is awaiting moderator approval.",
        "返信を送信しました。モデレーターの承認をお待ちください。",
    ),
    (
        "forum.reported",
        "Thank you. Your report has been submitted to the moderators.",
        "ありがとうございます。報告をモデレーターに送信しました。",
    ),
    (
        "forum.no_threads",
        "No threads in this category yet. Be the first to start a conversation!",
        "このカテゴリにはまだスレッドがありません。最初の話題を始めましょう。",
    ),
    ("forum.start_thread", "Start a New Thread", "新しいスレッドを作成"),
    ("forum.thread_title", "Thread Title", "スレッドのタイトル"),
    (
        "forum.thread_title_hint",
        "Summarize your question or topic...",
        "質問や話題を簡潔にまとめてください…",
    ),
    ("forum.your_name", "Your Name / Handle", "お名前 / ハンドルネーム"),
    ("forum.anonymous", "Anonymous", "匿名"),
    ("forum.message_body", "Message Body", "本文"),
    (
        "forum.message_hint",
        "Write your post here...",
        "ここに投稿を書いてください…",
    ),
    ("forum.create_thread", "Create Thread", "スレッドを作成"),
    ("forum.report", "Report", "通報"),
    (
        "forum.report_reason",
        "Reason for reporting this post...",
        "この投稿を通報する理由…",
    ),
    (
        "forum.locked_banner",
        "🔒 This thread has been locked. New replies are not accepted.",
        "🔒 このスレッドはロックされています。新しい返信は受け付けていません。",
    ),
    ("forum.post_reply", "Post a Reply", "返信を投稿"),
    ("forum.reply_content", "Reply Content", "返信の内容"),
    (
        "forum.reply_hint",
        "Write your response...",
        "返信を書いてください…",
    ),
    ("forum.submit_reply", "Submit Reply", "返信を送信"),
    // --- カートのスクリプト（/shop/cart.js） ---
    ("js.added", "Added to cart ✓", "カートに追加しました ✓"),
    (
        "js.request_failed",
        "Request failed ({status})",
        "リクエストに失敗しました（{status}）",
    ),
    ("js.subtotal", "Subtotal", "小計"),
    ("js.total", "Total", "合計"),
    ("js.empty", "Your cart is empty.", "カートは空です。"),
    ("js.browse", "Browse products", "商品を見る"),
    ("js.quantity_of", "Quantity of {title}", "{title} の数量"),
    ("js.remove", "Remove", "削除"),
    (
        "js.checkout_unavailable",
        "Online checkout is not available on this site.",
        "このサイトではオンライン決済を利用できません。",
    ),
    ("js.coupon", "Coupon code", "クーポンコード"),
    ("js.apply", "Apply", "適用"),
    ("js.checkout", "Checkout", "ご注文手続き"),
    ("js.name", "Name", "お名前"),
    ("js.email", "Email", "メールアドレス"),
    ("js.postal_code", "Postal code", "郵便番号"),
    ("js.region", "Prefecture / State", "都道府県 / 州"),
    ("js.city", "City", "市区町村"),
    ("js.address", "Address", "住所"),
    ("js.building", "Building / Room", "建物名・部屋番号"),
    ("js.phone", "Phone", "電話番号"),
    ("js.placing", "Placing order…", "注文を送信しています…"),
    ("js.place_order", "Place order", "注文する"),
    ("js.status.pending_payment", "Awaiting payment", "お支払い待ち"),
    ("js.status.paid", "Paid", "お支払い済み"),
    ("js.status.shipped", "Shipped", "発送済み"),
    ("js.status.completed", "Completed", "完了"),
    ("js.status.cancelled", "Cancelled", "キャンセル"),
    ("js.order_number", "Order number", "注文番号"),
    (
        "js.confirming",
        "Confirming your payment… this page updates automatically.",
        "お支払いを確認しています…このページは自動で更新されます。",
    ),
    ("js.how_to_pay", "How to pay", "お支払い方法"),
    ("js.ship_to", "Ship to", "お届け先"),
    ("js.continue", "Continue shopping", "買い物を続ける"),
    (
        "js.order_not_found",
        "Order not found. Use the link from your confirmation.",
        "注文が見つかりません。確認メールのリンクからアクセスしてください。",
    ),
    // --- 同梱テーマ: cyber-neon ---
    ("theme.cyber.nav_terminal", "[ TERMINAL ]", "[ ターミナル ]"),
    ("theme.cyber.nav_archives", "[ ARCHIVES ]", "[ アーカイブ ]"),
    ("theme.cyber.nav_admin", "ADMIN MATRIX", "管理マトリクス"),
    ("theme.cyber.secured_by", "SECURED BY", "セキュリティ保護:"),
    (
        "theme.cyber.hero_subtitle",
        "// NEURAL PROTOCOL CONNECTED :: PORT 8080",
        "// ニューラルプロトコル接続完了 :: PORT 8080",
    ),
    ("theme.cyber.access_node", "[ ACCESS NODE ]", "[ ノードにアクセス ]"),
    (
        "theme.cyber.empty",
        "// NO POSTS LOCATED IN SECURE STORAGE",
        "// セキュアストレージに投稿が見つかりません",
    ),
    ("theme.cyber.pagination_newer", "[ ← NEWER ]", "[ ← 新しい記事 ]"),
    ("theme.cyber.pagination_older", "[ OLDER → ]", "[ 過去の記事 → ]"),
    ("theme.cyber.node_id", "// NODE ID: {slug}", "// ノードID: {slug}"),
    ("theme.cyber.timestamp", "TIMESTAMP: {date}", "タイムスタンプ: {date}"),
    ("theme.cyber.operator", "OPERATOR: {author}", "オペレーター: {author}"),
    ("theme.cyber.return_archives", "[ RETURN TO ARCHIVES ]", "[ アーカイブに戻る ]"),
    ("theme.cyber.404_subtitle", "// 404 :: NODE NOT FOUND", "// 404 :: ノードが見つかりません"),
    ("theme.cyber.404_title", "NULL VECTOR", "ヌルベクトル"),
    (
        "theme.cyber.404_desc",
        "The requested cybernetic node does not exist or has been wiped from memory.",
        "要求されたサイバネティックノードは存在しないか、メモリから消去されました。",
    ),
    ("theme.cyber.404_action", "[ RE-ENGAGE CORE SYSTEM ]", "[ コアシステムに再接続 ]"),

    // --- 同梱テーマ: emerald-luxury ---
    ("theme.lux.nav_atelier", "Atelier", "アトリエ"),
    ("theme.lux.nav_exhibits", "Exhibits", "展示作品"),
    ("theme.lux.nav_console", "Console", "管理コンソール"),
    (
        "theme.lux.footer_secured",
        "Form and Function Secured by <strong>AegisCMS</strong>.",
        "造形と機能を <strong>AegisCMS</strong> で保護。",
    ),
    (
        "theme.lux.hero_subtitle",
        "Haute Computation • Private Edition",
        "洗練のコンピューティング • プライベートエディション",
    ),
    ("theme.lux.view_exhibit", "View Exhibit", "展示作品を鑑賞"),
    (
        "theme.lux.empty",
        "No published works are currently on exhibition.",
        "現在展示中の公開作品はありません。",
    ),
    ("theme.lux.pagination_newer", "Newer Exhibits", "新しい展示作品"),
    ("theme.lux.pagination_older", "Older Exhibits", "過去の展示作品"),
    ("theme.lux.archived_on", "Archived {date}", "{date} 収蔵"),
    ("theme.lux.curated_by", "Curated by {author}", "キュレーター: {author}"),
    ("theme.lux.return_archives", "Return to Exhibition Archives", "展示アーカイブへ戻る"),
    ("theme.lux.404_subtitle", "Artifact Unlocated", "作品が見つかりません"),
    ("theme.lux.404_title", "The Void", "虚空"),
    (
        "theme.lux.404_desc",
        "The requested exhibition piece does not reside within this curated collection.",
        "要求された展示作品はこのコレクションに存在しません。",
    ),
    ("theme.lux.404_action", "Return to Atelier Entrance", "アトリエのエントランスへ戻る"),

    // --- 同梱テーマ: minimal-editorial ---
    ("theme.editorial.nav_journal", "Journal", "ジャーナル"),
    ("theme.editorial.nav_archives", "Archives", "アーカイブ"),
    ("theme.editorial.nav_studio", "Studio", "スタジオ"),
    (
        "theme.editorial.footer_powered",
        "Powered by <strong>AegisCMS</strong> Zero-Trust Engine.",
        "Powered by <strong>AegisCMS</strong> ゼロトラストエンジン",
    ),
    ("theme.editorial.hero_tag", "Editorial Perspectives", "エディトリアル・パースペクティブ"),
    ("theme.editorial.read_essay", "Read Essay", "エッセイを読む"),
    (
        "theme.editorial.empty",
        "No published articles yet. Visit the studio to author a new essay.",
        "公開された記事はまだありません。スタジオで新しいエッセイを執筆してください。",
    ),
    ("theme.editorial.pagination_newer", "Newer Essays", "新しいエッセイ"),
    ("theme.editorial.pagination_older", "Older Essays", "過去のエッセイ"),
    ("theme.editorial.published_on", "Published {date}", "{date} 公開"),
    ("theme.editorial.written_by", "Written by {author}", "執筆: {author}"),
    ("theme.editorial.return_archives", "Return to Journal Archives", "ジャーナル一覧に戻る"),
    ("theme.editorial.404_title", "Absent Text", "存在しないテキスト"),
    (
        "theme.editorial.404_desc",
        "The requested essay or page could not be found within this journal.",
        "要求されたエッセイまたはページはこのジャーナル内に見つかりませんでした。",
    ),
    ("theme.editorial.404_action", "Return to Front Page", "トップページに戻る"),

    // --- 同梱テーマ: roastery ---
    (
        "theme.roastery.notice",
        "Free shipping on orders over ¥5,000 ・ Ships within 3 days of roasting",
        "5,000円以上のご注文で送料無料 ・ 焙煎から3日以内に発送します",
    ),
    ("theme.roastery.nav_shop", "Shop", "ショップ"),
    ("theme.roastery.nav_journal", "Journal", "ジャーナル"),
    ("theme.roastery.nav_about", "About", "私たちについて"),
    ("theme.roastery.nav_cart", "Cart", "カート"),
    ("theme.roastery.footer_shop", "Online Shop", "オンラインショップ"),
    ("theme.roastery.footer_journal", "Journal", "ジャーナル"),
    ("theme.roastery.footer_about", "About Us", "私たちについて"),
    ("theme.roastery.footer_shipping", "Shipping & Payment", "配送・お支払い"),
    ("theme.roastery.eyebrow", "Small batch coffee roasters", "スモールバッチ・コーヒーロースター"),
    (
        "theme.roastery.hero_title",
        "Every morning cup,<br>freshly roasted.",
        "毎朝の一杯を、<br>焙煎したての豆で。",
    ),
    ("theme.roastery.view_products", "View Products", "商品を見る"),
    ("theme.roastery.about_us", "About Us", "私たちについて"),
    (
        "theme.roastery.feature1",
        "<strong>Ships within 3 days of roasting</strong><span>Roasted in small batches after your order.</span>",
        "<strong>焙煎から3日以内に発送</strong><span>ご注文を受けてから小さな釜で焙煎します。</span>",
    ),
    (
        "theme.roastery.feature2",
        "<strong>Free shipping over ¥5,000</strong><span>Flat rate of ¥650 nationwide for all other orders.</span>",
        "<strong>5,000円以上で送料無料</strong><span>それ以外は全国一律 650円です。</span>",
    ),
    (
        "theme.roastery.feature3",
        "<strong>10% off for first-time buyers</strong><span>Enter coupon <code>WELCOME10</code> at checkout.</span>",
        "<strong>はじめての方は10%オフ</strong><span>カートでクーポン <code>WELCOME10</code> を入力してください。</span>",
    ),
    ("theme.roastery.all_articles", "All stories", "すべての記事"),
    ("theme.roastery.empty", "No stories published yet.", "まだ記事がありません。"),
    ("theme.roastery.return_journal", "Back to Journal", "Journal に戻る"),
    (
        "theme.roastery.404_desc",
        "The page you are looking for has been moved or deleted, or is not yet published.",
        "お探しのページは移動または削除されたか、まだ公開されていません。",
    ),
    ("theme.roastery.404_action", "View Products", "商品一覧へ"),

    // --- 同梱テーマ: trailhead ---
    ("theme.trailhead.nav_news", "News", "お知らせ"),
    ("theme.trailhead.nav_guidelines", "Guidelines", "ガイドライン"),
    ("theme.trailhead.nav_forum", "Forum", "掲示板"),
    ("theme.trailhead.footer_forum", "Forum", "掲示板"),
    ("theme.trailhead.footer_news", "News", "お知らせ"),
    ("theme.trailhead.footer_about", "About Us", "このサイトについて"),
    ("theme.trailhead.footer_guidelines", "Posting Guidelines", "投稿ガイドライン"),
    ("theme.trailhead.eyebrow", "Hiking community", "ハイキングコミュニティ"),
    (
        "theme.trailhead.hero_title",
        "Share mountain stories,<br>together.",
        "山の話を、<br>みんなで。",
    ),
    ("theme.trailhead.view_forum", "Visit Forum", "掲示板を見る"),
    ("theme.trailhead.guidelines", "Posting Guidelines", "投稿ガイドライン"),
    (
        "theme.trailhead.feature1",
        "<strong>First Steps</strong><span>Questions about choosing trails or gear are always welcome.</span>",
        "<strong>はじめての山</strong><span>山選びや持ち物など、どんな質問も歓迎です。</span>",
    ),
    (
        "theme.trailhead.feature2",
        "<strong>Gear &amp; Equipment</strong><span>Share impressions of hiking boots, rainwear, and packs.</span>",
        "<strong>道具・装備</strong><span>靴・雨具・ザックの使い心地を共有しましょう。</span>",
    ),
    (
        "theme.trailhead.feature3",
        "<strong>Trail Reports</strong><span>Document your adventures and trail conditions for others.</span>",
        "<strong>山行記録</strong><span>歩いた山の様子や登山道の状況を残してください。</span>",
    ),
    ("theme.trailhead.news_title", "News", "お知らせ"),
    ("theme.trailhead.all_news", "All News", "すべてのお知らせ"),
    ("theme.trailhead.empty", "No announcements yet.", "まだお知らせがありません。"),
    ("theme.trailhead.return_news", "Back to News", "お知らせに戻る"),
    (
        "theme.trailhead.404_desc",
        "The page you are looking for has been moved or deleted, or is not yet published.",
        "お探しのページは移動または削除されたか、まだ公開されていません。",
    ),
    ("theme.trailhead.404_action", "To Forum", "掲示板へ"),
];

/// すべての文言（`(キー, en, ja)`）。テストと、訳し漏れの検査用
pub fn all() -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
    MESSAGES.iter().copied()
}

/// `locale` が選べる言語か
pub fn is_supported(locale: &str) -> bool {
    LOCALES.contains(&locale)
}

fn pick(entry: &(&'static str, &'static str, &'static str), locale: &str) -> &'static str {
    if locale == "ja" {
        entry.2
    } else {
        entry.1
    }
}

/// 差し込み前の文言。知らないキーは `None`、知らない言語は英語
pub fn lookup(locale: &str, key: &str) -> Option<&'static str> {
    MESSAGES
        .iter()
        .find(|entry| entry.0 == key)
        .map(|entry| pick(entry, locale))
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

/// 文言を作る。`params` に `n` があれば `key.one`（`n` が 1）か `key.other` を使い、`{名前}` を
/// パラメーターの値（HTML エスケープ済み）で置き換える。知らないキーはキーをそのまま返す
/// （訳し漏れが画面で見つかる）
pub fn translate(locale: &str, key: &str, params: &[(String, String)]) -> String {
    let plural = params.iter().find(|(name, _)| name == "n").map(|(_, n)| {
        if n == "1" {
            format!("{}.one", key)
        } else {
            format!("{}.other", key)
        }
    });
    let Some(template) = plural
        .as_deref()
        .and_then(|plural_key| lookup(locale, plural_key))
        .or_else(|| lookup(locale, key))
    else {
        return key.to_string();
    };
    let mut out = template.to_string();
    for (name, value) in params {
        out = out.replace(&format!("{{{}}}", name), &escape(value));
    }
    out
}

/// `prefix` で始まるキーの文言（キーから `prefix` を除いたもの、差し込み前）。
/// カートのスクリプトへ渡す（`bundle(locale, "js.")`）
pub fn bundle(locale: &str, prefix: &str) -> Vec<(&'static str, &'static str)> {
    MESSAGES
        .iter()
        .filter_map(|entry| {
            entry
                .0
                .strip_prefix(prefix)
                .map(|name| (name, pick(entry, locale)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テンプレートで `aria-label` / `placeholder` / `title` の値に使うキー
    const ATTRIBUTE_KEYS: &[&str] = &[
        "pagination.label",
        "search.label",
        "shop.label",
        "shop.quantity",
        "forum.label",
        "forum.total_replies",
        "forum.thread_title_hint",
        "forum.message_hint",
        "forum.reply_hint",
        "forum.report_reason",
        "forum.anonymous",
    ];

    fn params(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_every_message_has_text_in_every_locale() {
        let mut seen = std::collections::HashSet::new();
        for (key, en, ja) in MESSAGES {
            assert!(seen.insert(*key), "duplicate key {}", key);
            assert!(!en.is_empty() && !ja.is_empty(), "{}", key);
            // 差し込み位置は両方の言語で同じ
            let placeholders = |text: &str| {
                let mut found: Vec<String> = text
                    .split('{')
                    .skip(1)
                    .filter_map(|rest| rest.split('}').next().map(str::to_string))
                    .collect();
                found.sort();
                found
            };
            assert_eq!(placeholders(en), placeholders(ja), "{}", key);
            // 属性に入れる文言は、属性を壊す文字を含まない
            if ATTRIBUTE_KEYS.contains(key) {
                for text in [en, ja] {
                    assert!(!text.contains(['"', '<', '>']), "{}", key);
                }
            }
        }
        // 数で出し分ける文言は one と other が揃っている
        for (key, _, _) in MESSAGES {
            if let Some(base) = key.strip_suffix(".one") {
                assert!(
                    lookup("en", &format!("{}.other", base)).is_some(),
                    "{}",
                    key
                );
            }
            if let Some(base) = key.strip_suffix(".other") {
                assert!(lookup("en", &format!("{}.one", base)).is_some(), "{}", key);
            }
        }
    }

    #[test]
    fn test_translate_substitutes_escapes_and_pluralizes() {
        assert_eq!(translate("en", "nav.home", &[]), "Home");
        assert_eq!(translate("ja", "nav.home", &[]), "ホーム");
        // 知らない言語は英語
        assert_eq!(translate("fr", "nav.home", &[]), "Home");
        assert_eq!(
            translate("en", "comments.count", &params(&[("n", "1")])),
            "1 comment"
        );
        assert_eq!(
            translate("en", "comments.count", &params(&[("n", "3")])),
            "3 comments"
        );
        assert_eq!(
            translate("ja", "comments.count", &params(&[("n", "1")])),
            "1 件のコメント"
        );
        // 値は HTML エスケープされ、文言のマークアップはそのまま
        assert_eq!(
            translate(
                "en",
                "forum.started_by",
                &params(&[("name", "<b>x</b> & y")])
            ),
            "Started by <strong>&lt;b&gt;x&lt;/b&gt; &amp; y</strong>"
        );
        // 知らないキーはキーのまま
        assert_eq!(translate("en", "no.such.key", &[]), "no.such.key");
    }

    #[test]
    fn test_bundle_strips_the_prefix() {
        let en = bundle("en", "js.");
        assert!(en.contains(&("added", "Added to cart ✓")));
        assert!(en.iter().all(|(name, _)| !name.starts_with("js.")));
        assert!(bundle("ja", "js.").contains(&("remove", "削除")));
    }
}
