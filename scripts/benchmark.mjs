#!/usr/bin/env node
/**
 * AegisCMS HTTP パフォーマンスベンチマークスクリプト
 *
 * 外部依存なし（Node.js 18+ 標準ライブラリのみ）で動作し、
 * コアサーバーの主要エンドポイント（公開サイト、API、プレビュー、EC 見積もり）の
 * スループット (req/s) とレイテンシ分布 (p50, p90, p99) を高精度に計測します。
 *
 * 使い方:
 *   node scripts/benchmark.mjs [オプション]
 *
 * api / preview は Viewer 以上のセッションが必要なため、--token か --user を指定しない場合は計測しない
 * （401 の応答と、拒否を記録する監査ログの書き込みを測ることになるため）。
 *
 * オプション:
 *   --url <url>          サーバーのベース URL (既定: http://127.0.0.1:8080)
 *   --concurrency, -c    並行リクエスト数 (既定: 50)
 *   --duration, -d       各テストの計測時間(秒) (既定: 5)
 *   --warmup, -w         ウォームアップ時間(秒) (既定: 1)
 *   --target, -t         計測対象 (all | index | article | api | preview | quote | health)
 *   --token <id>         セッション ID（環境変数 AEGIS_BENCH_TOKEN でも可）
 *   --user <name>        このユーザーでログインしてセッションを作る（パスワードは環境変数 AEGIS_BENCH_PASSWORD）
 *   --auto-server        サーバーが起動していない場合に自動起動する
 *   --json               結果を JSON 形式で標準出力に出力する
 *   --help, -h           ヘルプを表示
 */

import http from 'node:http';
import https from 'node:https';
import { spawn } from 'node:child_process';
import { performance } from 'node:perf_hooks';

// ============================================================================
// 引数解析
// ============================================================================

function parseArgs() {
  const args = process.argv.slice(2);
  const opts = {
    url: 'http://127.0.0.1:8080',
    concurrency: 50,
    duration: 5,
    warmup: 1,
    target: 'all',
    autoServer: false,
    json: false,
    token: process.env.AEGIS_BENCH_TOKEN || '',
    user: process.env.AEGIS_BENCH_USER || '',
  };

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--help' || arg === '-h') {
      printHelp();
      process.exit(0);
    } else if (arg === '--url' && args[i + 1]) {
      opts.url = args[++i].replace(/\/$/, '');
    } else if ((arg === '--concurrency' || arg === '-c') && args[i + 1]) {
      opts.concurrency = Math.max(1, parseInt(args[++i], 10) || 50);
    } else if ((arg === '--duration' || arg === '-d') && args[i + 1]) {
      opts.duration = Math.max(1, parseFloat(args[++i]) || 5);
    } else if ((arg === '--warmup' || arg === '-w') && args[i + 1]) {
      opts.warmup = Math.max(0, parseFloat(args[++i]) || 1);
    } else if ((arg === '--target' || arg === '-t') && args[i + 1]) {
      opts.target = args[++i].toLowerCase();
    } else if (arg === '--token' && args[i + 1]) {
      opts.token = args[++i];
    } else if (arg === '--user' && args[i + 1]) {
      opts.user = args[++i];
    } else if (arg === '--auto-server') {
      opts.autoServer = true;
    } else if (arg === '--json') {
      opts.json = true;
    }
  }

  return opts;
}

function printHelp() {
  console.log(`
AegisCMS HTTP パフォーマンスベンチマークツール

使用法:
  node scripts/benchmark.mjs [オプション]

オプション:
  --url <URL>           テスト対象サーバー URL (既定: http://127.0.0.1:8080)
  -c, --concurrency <N> 並行リクエスト数 (既定: 50)
  -d, --duration <SEC>  各エンドポイントの測定秒数 (既定: 5)
  -w, --warmup <SEC>    各テスト前のウォームアップ秒数 (既定: 1)
  -t, --target <NAME>   実行対象 (all | index | article | api | preview | quote | health)
  --token <ID>          セッション ID (環境変数 AEGIS_BENCH_TOKEN でも可)
  --user <NAME>         このユーザーでログインする (パスワードは環境変数 AEGIS_BENCH_PASSWORD、
                        ユーザーは環境変数 AEGIS_BENCH_USER でも可)
  --auto-server         サーバー未起動時に自動起動・終了する
  --json                測定結果を JSON で出力
  -h, --help            このヘルプを表示

実行例:
  task dev                                 # ターミナル1でサーバー起動
  node scripts/benchmark.mjs               # ターミナル2で全エンドポイント計測
  node scripts/benchmark.mjs -c 100 -d 10  # 並行100、10秒間の高負荷テスト
  node scripts/benchmark.mjs -t index      # トップページのみ計測
  AEGIS_BENCH_PASSWORD=... node scripts/benchmark.mjs --user admin  # api / preview も計測

api / preview は Viewer 以上のセッションが必要です。--token / --user を指定しない場合は
この 2 つを計測しません (401 の応答と監査ログの書き込みを測ることになるため)。
`);
}

// ============================================================================
// HTTP 負荷測定コア
// ============================================================================

function createAgent(isHttps, concurrency) {
  const options = {
    keepAlive: true,
    keepAliveMsecs: 1000,
    maxSockets: concurrency * 2,
    maxFreeSockets: concurrency,
    timeout: 10000,
  };
  return isHttps ? new https.Agent(options) : new http.Agent(options);
}

/**
 * 指定したエンドポイントに対して並行リクエストを実行し、メトリクスを収集する
 */
async function runLoadTest({
  url,
  method = 'GET',
  headers = {},
  body = null,
  concurrency,
  durationSec,
  agent,
}) {
  const targetUrl = new URL(url);
  const isHttps = targetUrl.protocol === 'https:';
  const httpModule = isHttps ? https : http;

  const latencies = [];
  const statusCounts = {};
  let totalBytes = 0;
  let totalRequests = 0;
  let errorCount = 0;

  const reqHeaders = {
    Connection: 'keep-alive',
    'User-Agent': 'AegisCMS-Bench/1.0',
    Accept: '*/*',
    ...headers,
  };

  if (body) {
    reqHeaders['Content-Length'] = Buffer.byteLength(body);
  }

  const reqOptions = {
    method,
    hostname: targetUrl.hostname,
    port: targetUrl.port || (isHttps ? 443 : 80),
    path: targetUrl.pathname + targetUrl.search,
    headers: reqHeaders,
    agent,
  };

  const startTime = performance.now();
  const endTime = startTime + durationSec * 1000;

  let activeRequests = 0;
  let stopTriggered = false;

  return new Promise((resolve) => {
    function fireNext() {
      if (stopTriggered || performance.now() >= endTime) {
        stopTriggered = true;
        if (activeRequests === 0) {
          finish();
        }
        return;
      }

      activeRequests++;
      const reqStart = performance.now();

      const req = httpModule.request(reqOptions, (res) => {
        let bytesReceived = 0;
        res.on('data', (chunk) => {
          bytesReceived += chunk.length;
        });

        res.on('end', () => {
          const reqDuration = performance.now() - reqStart;
          latencies.push(reqDuration);
          totalBytes += bytesReceived;
          totalRequests++;

          const status = res.statusCode;
          statusCounts[status] = (statusCounts[status] || 0) + 1;

          activeRequests--;
          fireNext();
        });
      });

      req.on('error', () => {
        errorCount++;
        activeRequests--;
        fireNext();
      });

      req.setTimeout(5000, () => {
        req.destroy(new Error('Request timeout'));
      });

      if (body) {
        req.write(body);
      }
      req.end();
    }

    function finish() {
      const totalElapsedSec = (performance.now() - startTime) / 1000;
      latencies.sort((a, b) => a - b);

      const rps = totalRequests / totalElapsedSec;
      const mbps = totalBytes / (1024 * 1024) / totalElapsedSec;

      const p50 = percentile(latencies, 50);
      const p90 = percentile(latencies, 90);
      const p95 = percentile(latencies, 95);
      const p99 = percentile(latencies, 99);
      const min = latencies.length > 0 ? latencies[0] : 0;
      const max = latencies.length > 0 ? latencies[latencies.length - 1] : 0;
      const mean =
        latencies.length > 0
          ? latencies.reduce((a, b) => a + b, 0) / latencies.length
          : 0;

      resolve({
        totalRequests,
        durationSec: totalElapsedSec,
        rps,
        mbps,
        errorCount,
        statusCounts,
        latencies: {
          min,
          mean,
          p50,
          p90,
          p95,
          p99,
          max,
        },
      });
    }

    // 並行ワーカーを開始
    for (let i = 0; i < concurrency; i++) {
      fireNext();
    }
  });
}

function percentile(sorted, p) {
  if (sorted.length === 0) return 0;
  const index = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, Math.min(index, sorted.length - 1))];
}

// ============================================================================
// サーバー自動検出 / 自動起動
// ============================================================================

async function checkServerHealth(baseUrl) {
  return new Promise((resolve) => {
    const url = new URL(`${baseUrl}/api/v1/health`);
    const isHttps = url.protocol === 'https:';
    const httpModule = isHttps ? https : http;

    const req = httpModule.get(url, { timeout: 2000 }, (res) => {
      resolve(res.statusCode === 200);
    });
    req.on('error', () => resolve(false));
    req.on('timeout', () => {
      req.destroy();
      resolve(false);
    });
  });
}

async function startAutoServer() {
  console.log('🚀 サーバーを起動中 (cargo run --release -p aegis-server -- run)...');
  const serverProc = spawn('cargo', ['run', '--release', '-p', 'aegis-server', '--', 'run'], {
    stdio: 'ignore',
    detached: false,
  });

  serverProc.on('error', (err) => {
    console.error('サーバーの起動に失敗しました:', err);
    process.exit(1);
  });

  // 最大 30 秒間ヘルスチェックを待機
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    await sleep(1000);
    if (await checkServerHealth('http://127.0.0.1:8080')) {
      console.log('✅ サーバーが正常に起動しました (ポート 8080)');
      return serverProc;
    }
  }

  console.error('❌ サーバーの起動タイムアウト (30秒)');
  serverProc.kill();
  process.exit(1);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// ============================================================================
// テストシナリオ定義
// ============================================================================

/** `POST /api/v1/auth/session` でログインし、セッション ID を返す */
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

async function discoverArticleSlug(baseUrl, authHeaders) {
  try {
    const res = await fetch(`${baseUrl}/api/v1/contents/article?limit=1&status=published`, {
      headers: authHeaders,
    });
    if (res.ok) {
      const data = await res.json();
      if (data.contents && data.contents.length > 0 && data.contents[0].slug) {
        return data.contents[0].slug;
      }
    }
  } catch {
    // ignore
  }
  return 'hello-world';
}

function getScenarios(baseUrl, articleSlug, authHeaders) {
  return [
    {
      id: 'health',
      name: 'Health Check (Routing Baseline)',
      description: 'ルーティングとフレームワークの最小オーバーヘッド',
      url: `${baseUrl}/api/v1/health`,
      method: 'GET',
    },
    {
      id: 'index',
      name: 'Top Page (Public HTML)',
      description: 'トップページ描画（MiniJinja テンプレート + テーマ適用）',
      url: `${baseUrl}/`,
      method: 'GET',
    },
    {
      id: 'article',
      name: 'Article Page (AST + HTML Rewriter)',
      description: '記事ページ描画（DB 取得 + AST 展開 + lol-html リライタ）',
      url: `${baseUrl}/articles/${articleSlug}`,
      method: 'GET',
    },
    {
      id: 'api',
      name: 'Contents List API (DB + Relations)',
      description: 'コンテンツ一覧 API（SQLite クエリ + リレーション解決）',
      url: `${baseUrl}/api/v1/contents/article?limit=50`,
      method: 'GET',
      headers: authHeaders,
      requiresAuth: true,
    },
    {
      id: 'preview',
      name: 'Preview API (Live AST Pipeline)',
      description: '公開プレビュー生成（Markdown / AST → HTML サニタイズ）',
      url: `${baseUrl}/api/v1/preview`,
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...authHeaders },
      requiresAuth: true,
      body: JSON.stringify({
        title: 'Benchmark Preview Article',
        content_type: 'article',
        blocks: [
          { name: 'core/heading', attributes: { level: 2 }, text: 'Benchmark Title' },
          { name: 'core/paragraph', text: 'Testing AegisCMS high-speed live preview pipeline under heavy concurrency load.' },
          { name: 'core/code', attributes: { language: 'rust' }, text: 'fn main() { println!("Hello benchmark"); }' },
        ],
      }),
    },
    {
      id: 'quote',
      name: 'Shop Quote API (Commerce + Plugins)',
      description: 'カート見積もり（価格計算 + Wasm プラグインフック実行）',
      url: `${baseUrl}/api/v1/shop/quote`,
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        lines: [
          { product_id: 'sample-product-id', quantity: 2 },
          { product_id: 'sample-product-id-2', quantity: 1 },
        ],
      }),
    },
  ];
}

// ============================================================================
// メインルーチン
// ============================================================================

async function main() {
  const opts = parseArgs();
  let serverProcess = null;

  try {
    // 1. サーバーの確認
    const isOnline = await checkServerHealth(opts.url);
    if (!isOnline) {
      if (opts.autoServer) {
        serverProcess = await startAutoServer();
      } else {
        console.error(`❌ サーバー (${opts.url}) に接続できません。`);
        console.error('別ターミナルで `task dev` を実行するか、`--auto-server` オプションを指定してください。');
        process.exit(1);
      }
    }

    const isHttps = opts.url.startsWith('https:');
    const agent = createAgent(isHttps, opts.concurrency);

    let token = opts.token;
    if (!token && opts.user) {
      const password = process.env.AEGIS_BENCH_PASSWORD;
      if (!password) {
        console.error('❌ --user を使う場合は環境変数 AEGIS_BENCH_PASSWORD にパスワードを設定してください。');
        process.exit(1);
      }
      token = await login(opts.url, opts.user, password);
    }
    const authHeaders = token ? { Authorization: `Bearer ${token}` } : {};

    const articleSlug = await discoverArticleSlug(opts.url, authHeaders);
    const allScenarios = getScenarios(opts.url, articleSlug, authHeaders);

    let activeScenarios =
      opts.target === 'all'
        ? allScenarios
        : allScenarios.filter((s) => s.id === opts.target);

    if (activeScenarios.length === 0) {
      console.error(`無効なターゲットです: ${opts.target}`);
      console.error(`利用可能: all, ${allScenarios.map((s) => s.id).join(', ')}`);
      process.exit(1);
    }

    // 認証が要るシナリオをセッションなしで回すと、401 と監査ログ（access.denied）の書き込みを測ることになる
    if (!token) {
      const skipped = activeScenarios.filter((s) => s.requiresAuth);
      if (skipped.length > 0) {
        if (opts.target !== 'all') {
          console.error(`❌ ${opts.target} の計測にはセッションが必要です。--token または --user を指定してください。`);
          process.exit(1);
        }
        console.error(
          `⚠️  セッション未指定のため ${skipped.map((s) => s.id).join(', ')} を計測しません (--token / --user で計測できます)。`
        );
        activeScenarios = activeScenarios.filter((s) => !s.requiresAuth);
      }
    }

    if (!opts.json) {
      console.log('\n==============================================================================');
      console.log('⚡ AegisCMS Performance Benchmark Suite');
      console.log('==============================================================================');
      console.log(` Target Server : ${opts.url}`);
      console.log(` Concurrency   : ${opts.concurrency} concurrent connections`);
      console.log(` Duration      : ${opts.duration}s per endpoint (warmup: ${opts.warmup}s)`);
      console.log(` Scenarios     : ${activeScenarios.map((s) => s.id).join(', ')}`);
      console.log('==============================================================================\n');
    }

    const results = [];

    for (let i = 0; i < activeScenarios.length; i++) {
      const s = activeScenarios[i];
      if (!opts.json) {
        process.stdout.write(
          `[${i + 1}/${activeScenarios.length}] ${s.name.padEnd(38)} ... `
        );
      }

      // ウォームアップ
      if (opts.warmup > 0) {
        await runLoadTest({
          url: s.url,
          method: s.method,
          headers: s.headers,
          body: s.body,
          concurrency: Math.min(opts.concurrency, 10),
          durationSec: opts.warmup,
          agent,
        });
      }

      // 本計測
      const res = await runLoadTest({
        url: s.url,
        method: s.method,
        headers: s.headers,
        body: s.body,
        concurrency: opts.concurrency,
        durationSec: opts.duration,
        agent,
      });

      results.push({
        scenario: s.id,
        name: s.name,
        description: s.description,
        ...res,
      });

      if (!opts.json) {
        const rpsStr = Math.round(res.rps).toLocaleString().padStart(8);
        const p50Str = (res.latencies.p50.toFixed(2) + 'ms').padStart(8);
        const p99Str = (res.latencies.p99.toFixed(2) + 'ms').padStart(8);
        const failed = nonSuccessStatuses(res.statusCounts);
        const mark = failed ? '⚠️ ' : '✅';
        console.log(`${mark} ${rpsStr} req/s | p50: ${p50Str} | p99: ${p99Str}${failed ? ` | ${failed}` : ''}`);
      }
    }

    if (opts.json) {
      console.log(
        JSON.stringify(
          {
            timestamp: new Date().toISOString(),
            config: {
              url: opts.url,
              concurrency: opts.concurrency,
              durationSec: opts.duration,
            },
            results,
          },
          null,
          2
        )
      );
    } else {
      printReport(results, opts);
    }
  } finally {
    if (serverProcess) {
      console.log('\n🛑 自動起動したテストサーバーを停止中...');
      serverProcess.kill();
    }
  }
}

// ============================================================================
// レポート表示
// ============================================================================

/** 2xx / 3xx 以外の応答の内訳（例: "HTTP 401 x 4001"）。すべて成功なら空文字 */
function nonSuccessStatuses(statusCounts) {
  return Object.entries(statusCounts)
    .filter(([status]) => Number(status) >= 400)
    .map(([status, count]) => `HTTP ${status} x ${count}`)
    .join(', ');
}

function printReport(results, opts) {
  console.log('\n==============================================================================');
  console.log('📊 Benchmark Results Summary');
  console.log('==============================================================================');

  const headers = [
    'Endpoint / Scenario'.padEnd(32),
    'Req/s'.padStart(10),
    'p50 (ms)'.padStart(10),
    'p90 (ms)'.padStart(10),
    'p99 (ms)'.padStart(10),
    'Max (ms)'.padStart(10),
    'Errors'.padStart(8),
  ];
  console.log(headers.join(' | '));
  console.log('-'.repeat(headers.join(' | ').length));

  for (const r of results) {
    const row = [
      r.name.slice(0, 32).padEnd(32),
      Math.round(r.rps).toLocaleString().padStart(10),
      r.latencies.p50.toFixed(2).padStart(10),
      r.latencies.p90.toFixed(2).padStart(10),
      r.latencies.p99.toFixed(2).padStart(10),
      r.latencies.max.toFixed(2).padStart(10),
      r.errorCount.toString().padStart(8),
    ];
    console.log(row.join(' | '));
  }
  console.log('==============================================================================');

  const failed = results.filter((r) => nonSuccessStatuses(r.statusCounts));
  if (failed.length > 0) {
    console.log('\n⚠️  エラー応答を含むシナリオ (エンドポイント本来の処理を測れていません):');
    for (const r of failed) {
      console.log(` - ${r.scenario}: ${nonSuccessStatuses(r.statusCounts)}`);
    }
  }

  console.log('\n💡 備考・アドバイス:');
  console.log(' - リリースビルド (`cargo build --release -p aegis-server`) を使用すると大幅にスループットが向上します。');
  console.log(' - Wasm プラグイン単体のプロファイルは `task bench:plugins` で段階別所要時間・同時実行数を測定できます。');
  console.log(' - 詳細なレイテンシ分布を計測する場合は `oha` ツール (`oha -c 50 -z 5s <URL>`) の併用も可能です。\n');
}

main().catch((err) => {
  console.error('\n致命的なエラーが発生しました:', err);
  process.exit(1);
});
