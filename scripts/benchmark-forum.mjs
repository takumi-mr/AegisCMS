#!/usr/bin/env node
/**
 * AegisCMS 掲示板（[forum]）の負荷ベンチマーク
 *
 * 専用の設定（config/bench-forum.toml: SQLite・監査ログなし・連投制限なし）でサーバーを起動し、
 * 掲示板のデータを API で投入してから、並行数を段階的に上げながら公開ページと投稿を計測します。
 * 「どれぐらいのリクエストまで耐えられるか」は、各シナリオで p99 が目標（--slo-p99）以内かつ
 * エラーなしで捌けた最大のスループットを表にして出します。
 *
 * 使い方:
 *   node scripts/benchmark-forum.mjs [オプション]
 *
 * オプション:
 *   --levels <n,n,...>   並行数の段階 (既定: 10,50,100,200,400)
 *   --duration, -d       各段階の計測時間(秒) (既定: 5)
 *   --threads <n>        投入するスレッド数 (既定: 200)
 *   --hot-posts <n>      人気スレッド 1 つのレス数 (既定: 1000)
 *   --slo-p99 <ms>       「耐えられた」とみなす p99 の上限 (既定: 500)
 *   --config <path>      サーバーの設定 (既定: config/bench-forum.toml)
 *   --keep-db            終了時にデータベースファイルを消さない
 *   --json               結果を JSON で出力
 *
 * 事前に `cargo build --release -p aegis-server` が必要です。
 * 負荷をかける側（Node.js）も同じマシンの CPU を使うので、数値は「この環境での目安」です。
 */

import http from 'node:http';
import fs from 'node:fs';
import { spawn, spawnSync } from 'node:child_process';
import { performance } from 'node:perf_hooks';

// ============================================================================
// 引数
// ============================================================================

function parseArgs() {
  const args = process.argv.slice(2);
  const opts = {
    levels: [10, 50, 100, 200, 400],
    duration: 5,
    threads: 200,
    hotPosts: 1000,
    sloP99: 500,
    config: 'config/bench-forum.toml',
    keepDb: false,
    json: false,
  };
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--levels' && args[i + 1]) {
      opts.levels = args[++i]
        .split(',')
        .map((n) => parseInt(n, 10))
        .filter((n) => n > 0);
    } else if ((arg === '--duration' || arg === '-d') && args[i + 1]) {
      opts.duration = Math.max(1, parseFloat(args[++i]) || 5);
    } else if (arg === '--threads' && args[i + 1]) {
      opts.threads = Math.max(1, parseInt(args[++i], 10) || 200);
    } else if (arg === '--hot-posts' && args[i + 1]) {
      opts.hotPosts = Math.max(1, parseInt(args[++i], 10) || 1000);
    } else if (arg === '--slo-p99' && args[i + 1]) {
      opts.sloP99 = Math.max(1, parseFloat(args[++i]) || 500);
    } else if (arg === '--config' && args[i + 1]) {
      opts.config = args[++i];
    } else if (arg === '--keep-db') {
      opts.keepDb = true;
    } else if (arg === '--json') {
      opts.json = true;
    } else if (arg === '--help' || arg === '-h') {
      console.log(fs.readFileSync(new URL(import.meta.url), 'utf8').split('*/')[0]);
      process.exit(0);
    }
  }
  return opts;
}

const BIN =
  process.platform === 'win32'
    ? '.\\target\\release\\aegis-server.exe'
    : './target/release/aegis-server';

// ============================================================================
// HTTP
// ============================================================================

function readConfigValue(path, key) {
  const text = fs.readFileSync(path, 'utf8');
  const match = text.match(new RegExp(`^${key}\\s*=\\s*"?([^"\\n]+)"?`, 'm'));
  return match ? match[1].trim() : '';
}

function request(port, { method = 'GET', path, headers = {}, body = null }) {
  return new Promise((resolve, reject) => {
    const req = http.request({ hostname: '127.0.0.1', port, path, method, headers }, (res) => {
      let data = '';
      res.on('data', (c) => (data += c));
      res.on('end', () => resolve({ status: res.statusCode, headers: res.headers, data }));
    });
    req.on('error', reject);
    if (body) req.write(body);
    req.end();
  });
}

async function waitForServer(port) {
  for (let i = 0; i < 150; i++) {
    try {
      const res = await request(port, { path: '/api/v1/health' });
      if (res.status === 200) return;
    } catch {
      // まだ起動していない
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error('サーバーが起動しませんでした');
}

async function api(port, token, method, path, json) {
  const body = json === undefined ? null : JSON.stringify(json);
  const res = await request(port, {
    method,
    path,
    headers: {
      Authorization: `Bearer ${token}`,
      ...(body ? { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body) } : {}),
    },
    body,
  });
  if (res.status >= 300) {
    throw new Error(`${method} ${path} -> HTTP ${res.status}: ${res.data.slice(0, 200)}`);
  }
  return res.data ? JSON.parse(res.data) : {};
}

// ============================================================================
// データの投入
// ============================================================================

/** `fn` を `concurrency` 並行で `count` 回呼ぶ */
async function pool(count, concurrency, fn) {
  let next = 0;
  await Promise.all(
    Array.from({ length: concurrency }, async () => {
      while (next < count) {
        const i = next++;
        await fn(i);
      }
    })
  );
}

const SENTENCES = [
  '先週末に近くの山へ行ってきました。天気が良く、見晴らしも最高でした。',
  'この装備で冬の低山は問題ないでしょうか。経験者の方のご意見をお願いします。',
  'ルートの状況について、最近歩いた方がいたら教えてください。',
  'Thanks for the report! Does anyone know if the trail is still closed after the storm?',
  '写真を撮ってきたので後で上げます。<script>alert(1)</script> のような文字も一応エスケープされるはず。',
];

function postBody(n) {
  const lines = [];
  for (let i = 0; i < 1 + (n % 3); i++) lines.push(SENTENCES[(n + i) % SENTENCES.length]);
  return lines.join('\n\n');
}

async function seed(port, token, opts) {
  const categories = [];
  for (let c = 0; c < 5; c++) {
    const cat = await api(port, token, 'POST', '/api/v1/forum/categories', {
      slug: `cat-${c}`,
      name: `カテゴリ ${c}`,
      description: `ベンチマーク用のカテゴリ ${c}`,
      position: c,
    });
    categories.push(cat.category ?? cat);
  }

  const threads = [];
  await pool(opts.threads, 8, async (i) => {
    const category = categories[i % categories.length];
    const res = await api(port, token, 'POST', '/api/v1/forum/threads', {
      category_id: category.id,
      title: `ベンチマーク用のスレッド ${i}`,
      body: postBody(i),
      author_name: 'bench',
    });
    const thread = res.thread ?? res;
    threads[i] = { id: thread.id, category: category.slug };
  });

  // 人気スレッド（0 番）にレスを積む。一覧の深いページと、レスの多いスレッドの表示を測るため
  const hot = threads[0];
  for (let n = 1; n < opts.hotPosts; n += 50) {
    const batch = Math.min(50, opts.hotPosts - n);
    await pool(batch, 1, async (j) => {
      await api(port, token, 'POST', '/api/v1/forum/posts', {
        thread_id: hot.id,
        body: postBody(n + j),
        author_name: 'bench',
      });
    });
  }
  // 他のスレッドにも少しずつレスを付ける
  await pool(opts.threads - 1, 8, async (i) => {
    const t = threads[i + 1];
    for (let k = 0; k < 3; k++) {
      await api(port, token, 'POST', '/api/v1/forum/posts', {
        thread_id: t.id,
        body: postBody(i + k),
        author_name: 'bench',
      });
    }
  });

  return { categories, threads, hot };
}

// ============================================================================
// 負荷
// ============================================================================

function percentile(sorted, p) {
  if (sorted.length === 0) return 0;
  return sorted[Math.max(0, Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1))];
}

/** `next()` が返すリクエストを `concurrency` 並行で `durationSec` 秒送り続ける */
function runLoad(port, { next, concurrency, durationSec }) {
  const agent = new http.Agent({ keepAlive: true, maxSockets: concurrency, maxFreeSockets: concurrency });
  const latencies = [];
  const statuses = {};
  let errors = 0;
  let bytes = 0;
  let active = 0;
  const started = performance.now();
  const end = started + durationSec * 1000;

  return new Promise((resolve) => {
    const finish = () => {
      agent.destroy();
      const elapsed = (performance.now() - started) / 1000;
      latencies.sort((a, b) => a - b);
      const total = latencies.length;
      resolve({
        concurrency,
        requests: total,
        rps: total / elapsed,
        errors,
        statuses,
        mbps: bytes / 1048576 / elapsed,
        p50: percentile(latencies, 50),
        p90: percentile(latencies, 90),
        p99: percentile(latencies, 99),
        max: latencies[total - 1] ?? 0,
      });
    };

    const fire = () => {
      if (performance.now() >= end) {
        if (active === 0) finish();
        return;
      }
      active++;
      const r = next();
      const headers = { ...(r.headers ?? {}) };
      if (r.body) headers['Content-Length'] = Buffer.byteLength(r.body);
      const t0 = performance.now();
      const req = http.request(
        { hostname: '127.0.0.1', port, path: r.path, method: r.method ?? 'GET', headers, agent },
        (res) => {
          res.on('data', (c) => (bytes += c.length));
          res.on('end', () => {
            latencies.push(performance.now() - t0);
            statuses[res.statusCode] = (statuses[res.statusCode] || 0) + 1;
            active--;
            fire();
          });
        }
      );
      req.on('error', () => {
        errors++;
        active--;
        fire();
      });
      req.setTimeout(10000, () => req.destroy(new Error('timeout')));
      if (r.body) req.write(r.body);
      req.end();
    };

    for (let i = 0; i < concurrency; i++) fire();
  });
}

let counter = 0;
const rnd = (n) => Math.floor(Math.random() * n);

function postForm(path) {
  const body = new URLSearchParams({
    author_name: `bench${rnd(1000)}`,
    body: postBody(counter++),
  }).toString();
  return {
    method: 'POST',
    path,
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body,
  };
}

function scenarios(data, opts) {
  const { categories, threads, hot } = data;
  const get = (path) => ({ path });
  // 書き込みはホットスレッド以外に散らす（1 つのスレッドへの集中は別のシナリオで測る）
  const spread = () => threads[1 + rnd(threads.length - 1)];
  const lastHotPage = Math.ceil(opts.hotPosts / 50);

  return [
    {
      id: 'index',
      name: '掲示板トップ',
      note: 'GET /forum（カテゴリ一覧 + 最近のスレッド）',
      next: () => get('/forum'),
    },
    {
      id: 'category',
      name: 'スレッド一覧',
      note: `GET /forum/{slug}（${Math.round(opts.threads / categories.length)} スレッド/カテゴリ、ランダムなカテゴリ）`,
      next: () => get(`/forum/${categories[rnd(categories.length)].slug}`),
    },
    {
      id: 'thread',
      name: 'スレッド表示',
      note: 'GET /forum/{slug}/{id}（数レスのスレッド、ランダム）',
      next: () => {
        const t = spread();
        return get(`/forum/${t.category}/${t.id}`);
      },
    },
    {
      id: 'hot',
      name: '人気スレッド',
      note: `GET /forum/{slug}/{id}?page=N（${opts.hotPosts} レスのスレッド、ランダムなページ）`,
      next: () => get(`/forum/${hot.category}/${hot.id}?page=${1 + rnd(lastHotPage)}`),
    },
    {
      id: 'post',
      name: '返信の投稿',
      note: 'POST /forum/{slug}/{id}/posts（匿名のフォーム、スレッドに分散）',
      next: () => {
        const t = spread();
        return postForm(`/forum/${t.category}/${t.id}/posts`);
      },
    },
    {
      id: 'hotpost',
      name: '1 スレッドに集中して投稿',
      note: 'POST /forum/{slug}/{id}/posts（全員が同じスレッドに書く）',
      next: () => postForm(`/forum/${hot.category}/${hot.id}/posts`),
    },
    {
      id: 'mix',
      name: '閲覧 95% + 投稿 5%',
      note: 'トップ・一覧・スレッドの閲覧と、返信の投稿の混在',
      next: () => {
        const r = Math.random();
        const t = spread();
        if (r < 0.05) return postForm(`/forum/${t.category}/${t.id}/posts`);
        if (r < 0.1) return get('/forum');
        if (r < 0.3) return get(`/forum/${categories[rnd(categories.length)].slug}`);
        return get(`/forum/${t.category}/${t.id}`);
      },
    },
  ];
}

// ============================================================================
// メイン
// ============================================================================

async function main() {
  const opts = parseArgs();
  const port = parseInt(readConfigValue(opts.config, 'port'), 10);
  const dbUrl = readConfigValue(opts.config, 'url');
  const dbFile = (dbUrl.match(/^sqlite:(?:\/\/)?([^?]+)/) || [])[1];
  const log = (...a) => (opts.json ? console.error(...a) : console.log(...a));

  if (!fs.existsSync(BIN)) {
    console.error(`${BIN} がありません。先に cargo build --release -p aegis-server を実行してください。`);
    process.exit(1);
  }

  const removeDb = () => {
    if (!dbFile) return;
    for (const suffix of ['', '-shm', '-wal']) fs.rmSync(dbFile + suffix, { force: true });
  };
  removeDb();

  log('データベースを初期化しています...');
  const init = spawnSync(
    BIN,
    ['--config', opts.config, 'init', '--site-title', 'Forum Bench', '-u', 'admin', '--admin-password', 'password123'],
    { encoding: 'utf8' }
  );
  if (init.status !== 0) {
    console.error(init.stdout, init.stderr);
    process.exit(1);
  }

  const server = spawn(BIN, ['--config', opts.config, 'run'], { stdio: 'ignore' });
  const stop = () => {
    server.kill();
    if (!opts.keepDb) setTimeout(removeDb, 1000);
  };
  process.on('SIGINT', () => {
    stop();
    process.exit(130);
  });

  try {
    await waitForServer(port);
    const login = await request(port, {
      method: 'POST',
      path: '/api/v1/auth/session',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username: 'admin', password: 'password123' }),
    });
    if (login.status !== 200 && login.status !== 201) throw new Error(`ログイン失敗 (HTTP ${login.status})`);
    const token = JSON.parse(login.data).id;

    log(`データを投入しています（${opts.threads} スレッド、人気スレッド ${opts.hotPosts} レス）...`);
    const t0 = performance.now();
    const data = await seed(port, token, opts);
    log(`  完了 (${((performance.now() - t0) / 1000).toFixed(1)} 秒)\n`);

    const results = [];
    for (const scenario of scenarios(data, opts)) {
      log(`■ ${scenario.name} — ${scenario.note}`);
      // ウォームアップ
      await runLoad(port, { next: scenario.next, concurrency: 20, durationSec: 1 });
      const rows = [];
      for (const concurrency of opts.levels) {
        const r = await runLoad(port, { next: scenario.next, concurrency, durationSec: opts.duration });
        rows.push(r);
        const failed = r.errors + Object.entries(r.statuses).filter(([s]) => +s >= 400).reduce((n, [, c]) => n + c, 0);
        log(
          `  c=${String(concurrency).padStart(4)}  ${r.rps.toFixed(0).padStart(6)} req/s  ` +
            `p50 ${r.p50.toFixed(1).padStart(7)}ms  p99 ${r.p99.toFixed(1).padStart(8)}ms  ` +
            `max ${r.max.toFixed(0).padStart(6)}ms  ` +
            `${failed ? `失敗 ${failed}  ` : ''}${JSON.stringify(r.statuses)}`
        );
      }
      const ok = rows.filter(
        (r) =>
          r.errors === 0 &&
          Object.entries(r.statuses).every(([s]) => +s < 400) &&
          r.p99 <= opts.sloP99
      );
      const best = ok.length ? ok.reduce((a, b) => (b.rps > a.rps ? b : a)) : null;
      results.push({ ...scenario, next: undefined, rows, best });
      log(
        best
          ? `  → p99 ${opts.sloP99}ms 以内で捌けた最大: ${best.rps.toFixed(0)} req/s（並行 ${best.concurrency}）\n`
          : `  → 最小の並行数でも p99 ${opts.sloP99}ms 以内に収まりませんでした\n`
      );
    }

    if (opts.json) {
      console.log(JSON.stringify({ opts, results }, null, 2));
    } else {
      console.log(`\n=== まとめ（p99 ≤ ${opts.sloP99}ms・エラーなしで捌けた最大のスループット）===`);
      console.log('| シナリオ | 最大 req/s | そのときの p50 / p99 | 並行数 |');
      console.log('| :--- | ---: | ---: | ---: |');
      for (const r of results) {
        const b = r.best;
        console.log(
          b
            ? `| ${r.name} | ${b.rps.toFixed(0)} | ${b.p50.toFixed(1)}ms / ${b.p99.toFixed(1)}ms | ${b.concurrency} |`
            : `| ${r.name} | - | - | - |`
        );
      }
    }
  } finally {
    stop();
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
