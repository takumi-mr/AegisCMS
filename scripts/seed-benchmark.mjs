#!/usr/bin/env node
/**
 * AegisCMS ベンチマーク用データ投入スクリプト
 *
 * 外部依存なし（Node.js 18+ 標準ライブラリのみ）。起動中のサーバーへ HTTP API 経由で
 * カテゴリと記事を投入し、件数の多い状態で `scripts/benchmark.mjs` を測れるようにします。
 * 記事は見出し・段落・コード・リストなど約 10 ブロック（約 4KB）で、カテゴリへのリレーションを持ちます。
 * 10 件に 1 件（`<prefix>-article-N` の N を 10 で割った余りが 9）は下書きです。
 * 投入にかかった時間（件数/秒）も表示します（記事の作成と監査ログの書き込みを含む）。
 *
 * 使い方:
 *   AEGIS_BENCH_PASSWORD=... node scripts/seed-benchmark.mjs --user admin [オプション]
 *
 * オプション:
 *   --url <url>          サーバーのベース URL (既定: http://127.0.0.1:8080)
 *   --token <id>         セッション ID（環境変数 AEGIS_BENCH_TOKEN でも可）
 *   --user <name>        Editor 以上のユーザーでログインする（パスワードは環境変数 AEGIS_BENCH_PASSWORD）
 *   --count, -n          投入する記事の件数 (既定: 10000)
 *   --categories         カテゴリの件数 (既定: 20)
 *   --concurrency, -c    並行リクエスト数 (既定: 16)
 *   --prefix <name>      slug の接頭辞 (既定: bench)。同じ接頭辞で 2 回目を流すと slug が重複して失敗する
 *   --help, -h           ヘルプを表示
 */

function parseArgs() {
  const args = process.argv.slice(2);
  const opts = {
    url: 'http://127.0.0.1:8080',
    token: process.env.AEGIS_BENCH_TOKEN || '',
    user: process.env.AEGIS_BENCH_USER || '',
    count: 10000,
    categories: 20,
    concurrency: 16,
    prefix: 'bench',
  };

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--help' || arg === '-h') {
      printHelp();
      process.exit(0);
    } else if (arg === '--url' && args[i + 1]) {
      opts.url = args[++i].replace(/\/$/, '');
    } else if (arg === '--token' && args[i + 1]) {
      opts.token = args[++i];
    } else if (arg === '--user' && args[i + 1]) {
      opts.user = args[++i];
    } else if ((arg === '--count' || arg === '-n') && args[i + 1]) {
      opts.count = Math.max(1, parseInt(args[++i], 10) || 10000);
    } else if (arg === '--categories' && args[i + 1]) {
      opts.categories = Math.max(1, parseInt(args[++i], 10) || 20);
    } else if ((arg === '--concurrency' || arg === '-c') && args[i + 1]) {
      opts.concurrency = Math.max(1, parseInt(args[++i], 10) || 16);
    } else if (arg === '--prefix' && args[i + 1]) {
      opts.prefix = args[++i];
    }
  }

  return opts;
}

function printHelp() {
  console.log(`
AegisCMS ベンチマーク用データ投入ツール

使用法:
  AEGIS_BENCH_PASSWORD=... node scripts/seed-benchmark.mjs --user <NAME> [オプション]

オプション:
  --url <URL>           サーバー URL (既定: http://127.0.0.1:8080)
  --token <ID>          セッション ID (環境変数 AEGIS_BENCH_TOKEN でも可)
  --user <NAME>         Editor 以上のユーザーでログインする (パスワードは環境変数 AEGIS_BENCH_PASSWORD)
  -n, --count <N>       記事の件数 (既定: 10000)
  --categories <N>      カテゴリの件数 (既定: 20)
  -c, --concurrency <N> 並行リクエスト数 (既定: 16)
  --prefix <NAME>       slug の接頭辞 (既定: bench)
  -h, --help            このヘルプを表示

実行例:
  AEGIS_BENCH_PASSWORD=... node scripts/seed-benchmark.mjs --user admin            # 記事 1 万件
  AEGIS_BENCH_PASSWORD=... node scripts/seed-benchmark.mjs --user admin -n 100000  # 記事 10 万件
`);
}

async function login(baseUrl, username, password) {
  const res = await fetch(`${baseUrl}/api/v1/auth/session`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password }),
  });
  if (!res.ok) {
    throw new Error(`ログインに失敗しました (HTTP ${res.status})`);
  }
  const data = await res.json();
  return data.id;
}

// 再現できるよう、乱数は固定のシードから作る
let seed = 42;
function random() {
  seed = (seed * 1103515245 + 12345) % 2147483648;
  return seed / 2147483648;
}

const WORDS = (
  'rust wasm cms sandbox schema stream rewriter plugin cache latency throughput ' +
  'query index relation theme template render audit session security'
).split(' ');

function sentence(words) {
  const picked = Array.from({ length: words }, () => WORDS[Math.floor(random() * WORDS.length)]);
  return picked.join(' ') + '.';
}

function paragraph() {
  return Array.from({ length: 6 }, () => sentence(12)).join(' ');
}

function article(i, prefix, categoryIds) {
  const slug = `${prefix}-article-${i}`;
  return {
    title: `Article ${i}: ${sentence(5)}`,
    slug,
    status: i % 10 === 9 ? 'draft' : 'published',
    excerpt: sentence(20),
    category_id: categoryIds[i % categoryIds.length],
    custom_meta: { seo: { canonical_url: `https://example.com/articles/${slug}` } },
    blocks: [
      { name: 'core/heading', attributes: { level: 2 }, text: sentence(4) },
      { name: 'core/paragraph', text: paragraph() },
      { name: 'core/paragraph', text: paragraph() },
      { name: 'core/heading', attributes: { level: 3 }, text: sentence(4) },
      { name: 'core/paragraph', text: paragraph() },
      {
        name: 'core/code',
        attributes: { language: 'rust' },
        text: 'fn main() {\n    let v: Vec<u32> = (0..10).collect();\n    println!("{:?}", v);\n}',
      },
      {
        name: 'core/list',
        children: [0, 1, 2].map(() => ({ name: 'core/list-item', text: sentence(6) })),
      },
      { name: 'core/paragraph', text: paragraph() },
      { name: 'core/heading', attributes: { level: 3 }, text: sentence(4) },
      { name: 'core/paragraph', text: paragraph() },
    ],
  };
}

async function main() {
  const opts = parseArgs();

  let token = opts.token;
  if (!token && opts.user) {
    const password = process.env.AEGIS_BENCH_PASSWORD;
    if (!password) {
      console.error('❌ --user を使う場合は環境変数 AEGIS_BENCH_PASSWORD にパスワードを設定してください。');
      process.exit(1);
    }
    token = await login(opts.url, opts.user, password);
  }
  if (!token) {
    console.error('❌ 投入には Editor 以上のセッションが必要です。--token または --user を指定してください。');
    process.exit(1);
  }
  const headers = { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` };

  async function create(contentType, body) {
    const res = await fetch(`${opts.url}/api/v1/contents/${contentType}`, {
      method: 'POST',
      headers,
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      throw new Error(`${contentType} ${body.slug}: HTTP ${res.status} ${await res.text()}`);
    }
    return res.json();
  }

  const categoryIds = [];
  for (let i = 0; i < opts.categories; i++) {
    const category = await create('category', {
      title: `Category ${i}`,
      slug: `${opts.prefix}-category-${i}`,
      status: 'published',
      description: sentence(10),
    });
    categoryIds.push(category.id);
  }

  console.log(
    `📝 記事 ${opts.count.toLocaleString()} 件を投入中 (${opts.url}、並行 ${opts.concurrency}、カテゴリ ${opts.categories} 件)...`
  );
  const start = Date.now();
  let next = 0;
  let created = 0;
  let failed = 0;
  const step = Math.max(1, Math.floor(opts.count / 10));

  async function worker() {
    while (next < opts.count) {
      const i = next++;
      try {
        await create('article', article(i, opts.prefix, categoryIds));
        created++;
      } catch (err) {
        failed++;
        if (failed <= 3) {
          console.error(`⚠️  ${err.message}`);
        }
      }
      const done = created + failed;
      if (done % step === 0) {
        const elapsed = (Date.now() - start) / 1000;
        console.log(`   ${done.toLocaleString()} / ${opts.count.toLocaleString()} (${elapsed.toFixed(1)}s)`);
      }
    }
  }

  await Promise.all(Array.from({ length: opts.concurrency }, worker));

  const elapsed = (Date.now() - start) / 1000;
  console.log(
    `✅ ${created.toLocaleString()} 件を ${elapsed.toFixed(1)} 秒で投入しました (${Math.round(created / elapsed)} 件/秒)` +
      (failed > 0 ? `、失敗 ${failed} 件` : '')
  );
  if (failed > 0) {
    process.exit(1);
  }
}

main().catch((err) => {
  console.error('\n致命的なエラーが発生しました:', err);
  process.exit(1);
});
