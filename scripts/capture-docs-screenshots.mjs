#!/usr/bin/env node
// docs/ に載せるスクリーンショットを撮る。サンプル（examples/）のデータを入れた使い捨てのサーバーを
// target/docs-shots/ に立て、Playwright で画面を開いて docs/images/ に PNG を書き出す。
// 事前に Admin Studio と aegis-server をビルドしておく（task docs:screenshots がやる）。
//
//   node scripts/capture-docs-screenshots.mjs [--only shop,forum,theme] [--filter <ファイル名の一部>]
//
// 撮る画面の定義は scenes/ ではなく下の SITES にまとめてある。UI が変わったら撮り直して差分をコミットする。
import { spawn, spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const { chromium } = createRequire(join(root, 'admin', 'package.json'))('@playwright/test');

const PORT = 18091;
const BASE = `http://127.0.0.1:${PORT}`;
const PASSWORD = 'sample-password';
const VIEWPORT = { width: 1920, height: 1080 };
const imagesDir = join(root, 'docs', 'images');
const workDir = join(root, 'target', 'docs-shots');
const isWindows = process.platform === 'win32';
const serverBin = join(root, 'target', 'debug', isWindows ? 'aegis-server.exe' : 'aegis-server');

const arg = (name) => {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : undefined;
};
const only = arg('only')?.split(',');
const filter = arg('filter');

// ---------------------------------------------------------------------------
// 撮影対象。`admin` は管理画面の中で行う操作、`public` は公開ページの path
// ---------------------------------------------------------------------------

const tab = (name) => async (page) => {
  await page.getByRole('navigation', { name: '画面' }).getByRole('button', { name, exact: true }).click();
  await page.waitForLoadState('networkidle');
};
const settings = (section) => async (page) => {
  await tab('設定')(page);
  await page.getByRole('navigation', { name: '設定の項目' }).getByRole('button', { name: section, exact: true }).click();
  await page.waitForLoadState('networkidle');
};

const json = { 'content-type': 'application/json' };

/** owner でログインして、管理 API 用のヘッダーを返す */
const adminHeaders = async () => {
  const login = await fetch(`${BASE}/api/v1/auth/session`, {
    method: 'POST',
    headers: json,
    body: JSON.stringify({ username: 'owner', password: PASSWORD }),
  });
  return { ...json, authorization: `Bearer ${(await login.json()).id}` };
};

/** API を呼んで JSON を返す。失敗したら本文つきで例外にする */
const call = async (method, path, headers, body) => {
  const res = await fetch(`${BASE}${path}`, { method, headers, body: body && JSON.stringify(body) });
  if (!res.ok) throw new Error(`${method} ${path} failed (${res.status}): ${await res.text()}`);
  return res.status === 204 ? null : res.json().catch(() => null);
};

/** 注文一覧・売上レポートが空にならないよう、顧客として注文を入れて、いくつかを入金済み・発送済みにする */
const placeShopOrders = async () => {
  const auth = await adminHeaders();
  const customers = [
    ['佐藤 花子', 'hanako@example.com', [['ethiopia-yirgacheffe', 2], ['drip-bag-assort', 1]], 'WELCOME10', 'shipped'],
    ['鈴木 一郎', 'ichiro@example.com', [['hand-grinder', 1]], undefined, 'paid'],
    ['高橋 美咲', 'misaki@example.com', [['house-blend-asanagi', 3]], undefined, 'completed'],
    ['田中 健太', 'kenta@example.com', [['seasonal-gift-set', 1], ['shigaraki-mug', 2]], 'COFFEE300', undefined],
    ['伊藤 さくら', 'sakura@example.com', [['arita-dripper', 1]], undefined, undefined],
  ];
  for (const [name, email, items, coupon, finalStatus] of customers) {
    const res = await fetch(`${BASE}/api/v1/shop/checkout`, {
      method: 'POST',
      headers: json,
      body: JSON.stringify({
        name,
        email,
        items: items.map(([product, quantity]) => ({ product, quantity })),
        coupon_code: coupon,
        shipping_address: { postal_code: '100-0001', region: '東京都', city: '千代田区', line1: '丸の内 1-1' },
      }),
    });
    if (res.status !== 201) throw new Error(`checkout failed (${res.status}): ${await res.text()}`);
    const { order } = await res.json();
    const steps = { paid: ['paid'], shipped: ['paid', 'shipped'], completed: ['paid', 'shipped', 'completed'] }[finalStatus] ?? [];
    for (const status of steps) {
      const r = await fetch(`${BASE}/api/v1/orders/${order.id}/status`, {
        method: 'POST',
        headers: auth,
        body: JSON.stringify({ status }),
      });
      if (!r.ok) throw new Error(`status ${status} failed (${r.status}): ${await r.text()}`);
    }
  }
};

const EMAILS = { 佐藤: 'sato', 鈴木: 'suzuki', 高橋: 'takahashi', 田中: 'tanaka', 'spam-bot': 'spam-bot' };

/** タグ・コメント・問い合わせ・アクセス解析のデータ（それぞれの管理画面が空にならないように） */
const seedExtras = async () => {
  const auth = await adminHeaders();

  const articles = ['how-to-brew-pour-over', 'roasting-schedule', 'autumn-lineup'];
  const tags = {};
  for (const [slug, name, description] of [
    ['hand-drip', 'ハンドドリップ', '自宅で淹れるコツ'],
    ['roasting', '焙煎', '焙煎所からのお知らせ'],
    ['new-crop', '新豆', '今季の新しい豆'],
    ['gear', '器具', 'ミルやドリッパーなど'],
  ]) {
    tags[slug] = (await call('POST', '/api/v1/tags', auth, { name, slug, description })).id;
  }
  const attach = { 'how-to-brew-pour-over': ['hand-drip', 'gear'], 'roasting-schedule': ['roasting'], 'autumn-lineup': ['new-crop', 'roasting'] };
  for (const [article, slugs] of Object.entries(attach)) {
    await call('PUT', `/api/v1/contents/article/${article}/tags`, auth, { tag_ids: slugs.map((t) => tags[t]) });
  }

  const posted = [];
  for (const [article, name, body] of [
    ['how-to-brew-pour-over', '佐藤', '蒸らしの時間を 30 秒にしたら、驚くほど甘く淹れられました。丁寧な解説をありがとうございます。'],
    ['how-to-brew-pour-over', '鈴木', 'ミルの粒度はどのくらいが目安でしょうか？中細挽きで試しています。'],
    ['autumn-lineup', '高橋', '新豆のグアテマラが楽しみです。入荷したらまたお買い物します。'],
    ['roasting-schedule', '田中', '焙煎日の前日までに注文すれば、翌日の焙煎に間に合いますか？'],
    ['autumn-lineup', 'spam-bot', 'Cheap watches!!! http://a.example http://b.example http://c.example'],
  ]) {
    const res = await fetch(`${BASE}/api/v1/contents/article/${article}/comments`, {
      method: 'POST',
      headers: json,
      body: JSON.stringify({ name, email: `${EMAILS[name]}@example.com`, body, website: '' }),
    });
    if (![201, 202].includes(res.status)) throw new Error(`comment failed (${res.status}): ${await res.text()}`);
    posted.push(name);
  }
  const { comments } = await call('GET', '/api/v1/comments?limit=50', auth);
  const byName = Object.fromEntries(comments.map((c) => [c.author_name ?? c.name, c]));
  for (const name of ['佐藤', '高橋']) await call('PATCH', `/api/v1/comments/${byName[name].id}`, auth, { status: 'approved' });
  await call('POST', `/api/v1/comments/${byName['佐藤'].id}/replies`, auth, {
    body: 'ありがとうございます。蒸らしは豆の鮮度でも変わるので、ぜひ試してみてください。',
  });
  await call('PATCH', `/api/v1/comments/${byName['spam-bot'].id}`, auth, { status: 'spam' });

  for (const [name, email, subject, message] of [
    ['山田 太郎', 'taro@example.com', '法人でのまとめ買いについて', '事務所用に、ハウスブレンドを毎月 2kg ほど購入したいです。定期配送は可能でしょうか。'],
    ['中村 優子', 'yuko@example.com', 'ギフトセットののし対応', '季節のギフトセットを贈答用にしたいのですが、のし紙は付けられますか。'],
    ['小林 誠', 'makoto@example.com', '注文のキャンセルについて', '昨日注文した手挽きコーヒーミルを、住所の入力誤りのため一度キャンセルしたいです。'],
  ]) {
    await call('POST', '/api/v1/contact/forms/default', json, { name, email, subject, message });
  }
  const { messages } = await call('GET', '/api/v1/contact/messages?limit=50', auth);
  const target = messages.find((m) => m.email === 'makoto@example.com') ?? messages[0];
  await call('PATCH', `/api/v1/contact/messages/${target.id}`, auth, { status: 'done' });

  // アクセス解析は「個々のアクセス」を持たず日ごとの集計だけを持つので、集計を直接入れる
  const { DatabaseSync } = await import('node:sqlite');
  const db = new DatabaseSync(join(workDir, 'shop', 'shop.db'));
  db.exec('PRAGMA busy_timeout = 15000');
  const insert = db.prepare(
    'INSERT INTO _aegis_analytics (day, dimension, value, pageviews, visitors) VALUES (?, ?, ?, ?, ?) ' +
      'ON CONFLICT(day, dimension, value) DO UPDATE SET pageviews = excluded.pageviews, visitors = excluded.visitors'
  );
  const share = [
    ['path', [['/', 0.34], ['/products', 0.22], ['/products/ethiopia-yirgacheffe', 0.12], ['/journal/how-to-brew-pour-over', 0.1], ['/products/house-blend-asanagi', 0.08], ['/cart', 0.07], ['/about', 0.07]]],
    ['referrer', [['google.com', 0.4], ['instagram.com', 0.28], ['t.co', 0.17], ['news.example.jp', 0.15]]],
    ['device', [['mobile', 0.58], ['desktop', 0.37], ['tablet', 0.05]]],
    ['utm_source', [['newsletter', 0.07], ['instagram', 0.05], ['x', 0.03]]],
    ['utm_campaign', [['autumn_sale', 0.08], ['new_crop', 0.05], ['gift_season', 0.02]]],
    ['utm_medium', [['email', 0.07], ['social', 0.08]]],
  ];
  // 過去 60 日（直前の 30 日が前期間比の比較対象になる）。直近ほど増えていく
  for (let i = 0; i < 60; i++) {
    const day = new Date(Date.now() - (59 - i) * 86400000).toISOString().slice(0, 10);
    const pageviews = 90 + ((i * 53) % 70) + i * 2;
    const visitors = Math.round(pageviews * 0.62);
    insert.run(day, 'site', '', pageviews, visitors);
    for (const [dimension, values] of share) {
      for (const [value, ratio] of values) {
        insert.run(day, dimension, value, Math.round(pageviews * ratio), Math.round(visitors * ratio));
      }
    }
  }
  db.close();
};

/** エディタで記事「ハンドドリップのいれ方」を開く */
const openArticle = async (page) => {
  await tab('エディタ')(page);
  await page.getByRole('button', { name: /はじめてのハンドドリップ/ }).first().click();
  await page.waitForLoadState('networkidle');
  await page.getByText('サーバーで描画中').waitFor({ state: 'detached' });
};

/** ブロックの追加ボタンと、並んだブロックが見えるところまでスクロールする */
const scrollToBlocks = async (page) => {
  await openArticle(page);
  await page.getByText('core/paragraph').first().scrollIntoViewIfNeeded();
  await page.getByRole('button', { name: '表', exact: true }).first().scrollIntoViewIfNeeded();
};

/** 記事に画像ブロックを足して、メディアライブラリを開く */
const openMediaLibrary = async (page) => {
  await openArticle(page);
  await page.getByRole('button', { name: '画像', exact: true }).first().click();
  await page.getByRole('button', { name: 'メディアライブラリから選ぶ' }).last().click();
  await page.getByText('メディアライブラリ').first().waitFor();
  await page.waitForLoadState('networkidle');
};

/** プロフィールを開いて、二段階認証の設定を始める（QR コードが出るところまで） */
const openTwoFactorSetup = async (page) => {
  await page.locator('button[title*="プロフィール"]').first().click();
  await page.getByRole('button', { name: '二段階認証を設定する' }).click();
  await page.getByText('手動入力用シークレット').waitFor();
  // QR コードとシークレットまで入る高さにする
  await page.setViewportSize({ ...VIEWPORT, height: 1500 });
};

const SITES = {
  shop: {
    config: 'examples/coffee-shop/aegis.toml',
    seed: 'examples/coffee-shop/seed.mjs',
    users: [['owner', 'admin', '店主']],
    user: 'owner',
    // コメント・問い合わせ・アクセス解析は既定で無効なので、このサイトでだけ有効にする
    extraConfig: `
[comments]
enabled = true
content_types = ["article"]

[comments.moderation]
require_approval = true

[contact]
enabled = true
locale = "ja"

[analytics]
enabled = true
`,
    after: async () => {
      await placeShopOrders();
      await seedExtras();
    },
    shots: [
      { file: 'commerce-products.png', admin: tab('商品') },
      { file: 'commerce-orders.png', admin: tab('注文') },
      { file: 'commerce-storefront.png', public: '/products' },
      { file: 'commerce-product.png', public: '/products/house-blend-asanagi' },
      {
        file: 'comments-admin.png',
        admin: async (page) => {
          await tab('コメント')(page);
          await page.locator('button, [role=tab]').filter({ hasText: /^すべて/ }).first().click();
        },
      },
      { file: 'comments-public.png', public: '/how-to-brew-pour-over', scrollTo: '#comments' },
      { file: 'contact-inbox.png', admin: tab('問い合わせ') },
      { file: 'contact-form.png', public: '/contact' },
      { file: 'analytics-dashboard.png', admin: tab('アクセス解析') },
      { file: 'search-results.png', public: '/search?q=ブレンド' },
      { file: 'tags-admin.png', admin: settings('タグ') },
      { file: 'audit-log.png', admin: settings('監査ログ') },
      { file: 'editor-article.png', admin: openArticle },
      { file: 'blocks-editor.png', admin: scrollToBlocks },
      { file: 'media-library.png', admin: openMediaLibrary },
      { file: 'users-admin.png', admin: settings('ユーザー') },
      { file: 'themes-admin.png', admin: settings('テーマ') },
      { file: 'plugins-admin.png', admin: settings('プラグイン') },
      { file: 'notifications-admin.png', admin: settings('通知') },
      { file: 'login.png', fresh: async (page) => page.goto(`${BASE}/admin/`) },
      { file: 'profile-2fa.png', admin: openTwoFactorSetup },
    ],
  },
  forum: {
    config: 'examples/hiking-forum/aegis.toml',
    seed: 'examples/hiking-forum/seed.mjs',
    users: [
      ['owner', 'admin', '管理人'],
      ['moderator', 'editor', 'モデレーター'],
    ],
    user: 'owner',
    shots: [
      { file: 'forum-admin.png', admin: tab('フォーラム') },
      { file: 'forum-public.png', public: '/forum' },
      {
        file: 'forum-thread.png',
        // スレッドの ID は投入のたびに変わるので、タイトルで探す
        public: async () => {
          const res = await fetch(`${BASE}/api/v1/forum/threads?category_slug=beginners&limit=100`);
          const body = await res.json();
          const threads = Array.isArray(body) ? body : body.threads;
          const thread = threads.find((t) => t.title === 'はじめての山におすすめの山はありますか？');
          return `/forum/beginners/${thread.id}`;
        },
      },
    ],
  },
};

// ---------------------------------------------------------------------------

const run = (command, args, env = {}) => {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', env: { ...process.env, ...env } });
  if (result.status !== 0) throw new Error(`"${command} ${args.join(' ')}" failed`);
};

/** サンプルの設定を、ポートとデータの置き場所だけ差し替えて target/docs-shots/<name>/ に書く */
const writeConfig = (name, site) => {
  const dir = join(workDir, name);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const rel = `target/docs-shots/${name}`;
  const text = readFileSync(join(root, site.config), 'utf8')
    .replace(/^port = \d+/m, `port = ${PORT}`)
    .replace(/examples\/[\w-]+\/(shop|site|forum)\.db/g, `${rel}/$1.db`)
    .replace(/examples\/[\w-]+\/uploads/g, `${rel}/uploads`)
    // 公開ページの組み込みの文言を日本語にする（ドキュメントが日本語なので）
    .replace(/^\[theme\]\r?\n/m, '[theme]\nlocale = "ja"\n')
    .concat(site.extraConfig ?? '');
  const path = join(dir, 'aegis.toml');
  writeFileSync(path, text);
  return path;
};

const waitForHealth = async () => {
  for (let i = 0; i < 120; i++) {
    try {
      if ((await fetch(`${BASE}/api/v1/health`)).ok) return;
    } catch {
      /* まだ起動していない */
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error('server did not become healthy');
};

const login = async (page, username) => {
  await page.goto(`${BASE}/admin/`);
  await page.locator('input[autocomplete="username"], input[name="username"]').first().fill(username);
  await page.locator('input[type="password"]').first().fill(PASSWORD);
  await page.locator('form button[type="submit"]').first().click();
  await page.locator('header').waitFor();
  await page.waitForLoadState('networkidle');
};

const captureSite = async (name, site) => {
  const config = writeConfig(name, site);
  for (const [username, role, displayName] of site.users) {
    run(serverBin, ['-c', config, 'user', 'add', username, '--role', role, '--display-name', displayName], {
      AEGIS_USER_PASSWORD: PASSWORD,
    });
  }
  const server = spawn(serverBin, ['-c', config, 'run'], {
    cwd: root,
    stdio: 'inherit',
    env: { ...process.env, RUST_LOG: 'warn' },
  });
  try {
    await waitForHealth();
    run(process.execPath, [site.seed, '--url', BASE, '--password', PASSWORD]);
    await site.after?.();

    const browser = await chromium.launch();
    const context = await browser.newContext({ viewport: VIEWPORT, locale: 'ja-JP', colorScheme: 'light' });
    const page = await context.newPage();
    await login(page, site.user);
    for (const shot of site.shots) {
      if (filter && !shot.file.includes(filter)) continue;
      if (shot.fresh) {
        // ログインしていない状態（ログイン画面）
        const anonymous = await browser.newContext({ viewport: VIEWPORT, locale: 'ja-JP', colorScheme: 'light' });
        const anonymousPage = await anonymous.newPage();
        await shot.fresh(anonymousPage);
        await anonymousPage.waitForLoadState('networkidle');
        await anonymousPage.screenshot({ path: join(imagesDir, shot.file) });
        await anonymous.close();
      } else if (shot.public) {
        const publicPage = await context.newPage();
        const path = typeof shot.public === 'function' ? await shot.public() : shot.public;
        await publicPage.goto(`${BASE}${path}`);
        await publicPage.waitForLoadState('networkidle');
        if (shot.scrollTo) {
          await publicPage.locator(shot.scrollTo).first().scrollIntoViewIfNeeded();
          await publicPage.evaluate(() => window.scrollBy(0, -120));
        }
        await publicPage.screenshot({ path: join(imagesDir, shot.file) });
        await publicPage.close();
      } else {
        // 前の画面のモーダルなどを残さないよう、毎回開き直す（再読み込みするとエディタから始まる）
        await page.setViewportSize(VIEWPORT);
        await page.reload();
        await page.locator('header').waitFor();
        await page.waitForLoadState('networkidle');
        await shot.admin(page);
        await page.waitForTimeout(300);
        await page.screenshot({ path: join(imagesDir, shot.file) });
      }
      console.log(`captured ${shot.file}`);
    }
    await browser.close();
  } finally {
    server.kill();
    await new Promise((r) => setTimeout(r, 500));
  }
};

/** Theme Builder（開発サーバー `theme dev` + Vite）。テーマはコピーに対して開くので、元のテーマは変わらない */
const captureThemeBuilder = async () => {
  const devPort = 18092;
  const builderPort = 15175;
  // Vite は localhost（IPv6 のことがある）で待ち受ける
  const builder = `http://localhost:${builderPort}`;
  const themesDir = join(workDir, 'theme');
  rmSync(themesDir, { recursive: true, force: true });
  mkdirSync(themesDir, { recursive: true });
  cpSync(join(root, 'themes', 'roastery'), join(themesDir, 'roastery'), { recursive: true });

  const children = [
    spawn(serverBin, ['theme', 'dev', join(themesDir, 'roastery'), '--port', String(devPort)], {
      cwd: root,
      stdio: 'inherit',
      env: { ...process.env, RUST_LOG: 'warn' },
    }),
    spawn('npx', ['vite', '--port', String(builderPort), '--strictPort'], {
      cwd: join(root, 'theme-builder'),
      stdio: 'inherit',
      shell: isWindows,
      env: { ...process.env, THEME_DEV_URL: `http://127.0.0.1:${devPort}` },
    }),
  ];
  try {
    for (let i = 0; ; i++) {
      try {
        if ((await fetch(builder)).ok) break;
      } catch {
        /* まだ起動していない */
      }
      if (i > 120) throw new Error('theme builder did not start');
      await new Promise((r) => setTimeout(r, 500));
    }
    const browser = await chromium.launch();
    const page = await (await browser.newContext({ viewport: VIEWPORT, locale: 'ja-JP', colorScheme: 'light' })).newPage();
    const frame = page.frameLocator('iframe[title="プレビュー"]');
    const open = async () => {
      await page.goto(builder);
      await page.locator('.cm-content').waitFor();
      await frame.locator('body *').first().waitFor();
      // 開発サーバーの変更監視が常に通信しているので networkidle は待たない
      await page.waitForTimeout(1500);
    };
    const themeBuilderShots = [
      { file: 'theme-builder.png', run: async () => {} },
      {
        file: 'theme-builder-manifest.png',
        run: async () => {
          await page.locator('.files .file', { hasText: 'theme.json' }).first().click();
          await page.waitForTimeout(500);
        },
      },
      {
        file: 'theme-builder-export.png',
        run: async () => {
          await page.getByRole('button', { name: 'パッケージ書き出し' }).click();
          await page.waitForTimeout(800);
        },
      },
    ];
    for (const shot of themeBuilderShots) {
      if (filter && !shot.file.includes(filter)) continue;
      await open();
      await shot.run();
      await page.waitForTimeout(500);
      await page.screenshot({ path: join(imagesDir, shot.file) });
      console.log(`captured ${shot.file}`);
    }
    await browser.close();
  } finally {
    for (const child of children) {
      // Windows では npx（シェル）の子の Vite まで止める必要がある
      if (isWindows) spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
      else child.kill();
    }
    await new Promise((r) => setTimeout(r, 500));
  }
};

mkdirSync(imagesDir, { recursive: true });
for (const [name, site] of Object.entries(SITES)) {
  if (only && !only.includes(name)) continue;
  await captureSite(name, site);
}
if (!only || only.includes('theme')) await captureThemeBuilder();
