#!/usr/bin/env node
// サンプル掲示板「やまびこ」のデータ（掲示板のカテゴリ・スレッド・レス、お知らせの記事、固定ページ）を API で投入する。
// 既にある slug・同じタイトルのスレッドはスキップするので、何度実行してもよい。依存パッケージは不要（Node.js 18 以上）。
//
//   node examples/hiking-forum/seed.mjs [--url http://127.0.0.1:8080] [--password ...]
//
// 管理者 owner とモデレーター moderator（editor）でログインする。パスワードは --password か
// 環境変数 AEGIS_SEED_PASSWORD（既定: sample-password。2 人とも同じ）。

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : fallback;
}

const BASE = arg('url', 'http://127.0.0.1:8080').replace(/\/$/, '');
const PASSWORD = arg('password', process.env.AEGIS_SEED_PASSWORD ?? 'sample-password');
const USERS = ['owner', 'moderator'];

// ---------------------------------------------------------------------------
// データ
// ---------------------------------------------------------------------------

const categories = [
  { slug: 'announcements', name: 'お知らせ', description: '運営からのお知らせとオフ会の案内です。' },
  { slug: 'beginners', name: 'はじめての山', description: '山選び・持ち物・歩き方など、はじめての方の質問はこちらへ。' },
  { slug: 'gear', name: '道具・装備', description: '靴・雨具・ザックなど、道具の選び方と使い心地。' },
  { slug: 'trip-reports', name: '山行記録', description: '歩いた山の様子や、登山道・山小屋の最新の状況。' },
];

// as: 'owner' / 'moderator' はログインして投稿する（Staff の印が付く）。省略すると未ログインで、author を名前にする
// （author を空にすると Anonymous）。reply_to はレス番号への返信。
// 一覧は最終投稿の新しい順なので、上に出したいスレッドを後に書く
const threads = [
  {
    category: 'trip-reports',
    title: '夏の北アルプス 情報交換（受付終了）',
    author: 'ライチョウ',
    body: 'この夏に北アルプスを歩いた方、山小屋や登山道の様子を教えてください。\n\n私は 8 月に立山から剱御前まで歩きました。雷鳥沢のキャンプ場は週末は満員でした。',
    replies: [
      { author: 'ゆる登山', body: '7 月末に燕岳に行きました。合戦尾根は朝 5 時の時点で駐車場がほぼ満車でした。' },
      { as: 'moderator', body: 'シーズンが終わったので、このスレッドはロックしました。情報ありがとうございました。秋の山行は「山行記録」に新しいスレッドを立ててください。' },
    ],
    lock: true,
  },
  {
    category: 'gear',
    title: '雨具はどれくらいのものを買えばいいですか',
    author: 'そらまめ',
    body: '雨の日は山に行かないつもりなので、コンビニのレインコートでもいいかなと思っています。やはり登山用のものを買うべきでしょうか？',
    replies: [
      { author: 'かもしか', body: '晴れの予報でも山の上では急に降ります。濡れると夏でも体が冷えるので、上下に分かれた登山用のレインウェアをおすすめします。' },
      { author: 'ゆる登山', body: '防水透湿素材（ゴアテックスなど）のものだと、歩いていても中が蒸れにくいです。風の強い日は防寒着にもなります。', reply_to: 1 },
      { author: 'そらまめ', body: 'ありがとうございます。上下セットのものを買って、晴れの日もザックに入れておくようにします。' },
    ],
  },
  {
    category: 'gear',
    title: '登山靴とトレランシューズ、どちらがいいですか',
    author: 'ハイカー見習い',
    body: '日帰りの低山が中心です。軽いトレランシューズでも大丈夫でしょうか。',
    replies: [
      { author: 'かもしか', body: '整備された低山ならトレランシューズで歩いている人も多いです。岩場や重い荷物のときは、足首まであるミッドカットの登山靴のほうが安心です。' },
      { author: '', body: '最初の一足は、お店で実際に履いてみるのが一番です。夕方の足がむくんだ時間に、登山用の厚い靴下で試してください。' },
    ],
  },
  {
    category: 'trip-reports',
    title: '10 月の燕岳、紅葉がきれいでした',
    author: 'ライチョウ',
    body: '先週末、中房温泉から燕岳を往復しました。合戦小屋のあたりはちょうど紅葉の見ごろでした。\n\n山頂付近は朝の気温が氷点下で、登山道の一部が凍っていました。軽アイゼンを持って行ってよかったです。',
    replies: [
      { author: 'ゆる登山', body: '写真で見るより実際の色はもっときれいなんでしょうね。燕山荘は混んでいましたか？' },
      { author: 'ライチョウ', body: '土曜日は満室でした。早めに予約しておくのがおすすめです。', reply_to: 2 },
      { as: 'moderator', body: '情報ありがとうございます。これからの時期は凍結が増えるので、皆さんも軽アイゼンや手袋の準備をお願いします。' },
    ],
  },
  {
    category: 'beginners',
    title: 'はじめての山におすすめの山はありますか？',
    author: 'そらまめ',
    body: '来月、友人と 2 人で初めて山に登ろうと思っています。東京から日帰りで行ける、初心者向けの山を教えてください。\n\n体力にはあまり自信がありません。',
    replies: [
      { author: 'かもしか', body: '高尾山はいかがでしょう。コースがいくつもあって、帰りはケーブルカーも使えます。' },
      { author: 'ゆる登山', body: '高尾山なら 6 号路（沢沿いの道）が涼しくておすすめです。混雑を避けるなら朝早めの出発がいいですよ。', reply_to: 1 },
      { as: 'moderator', body: 'はじめての山なら、登山届（登山計画書）を出しておくと安心です。歩く道と下山の予定時刻を家族にも伝えておいてください。' },
      { author: 'そらまめ', body: 'ありがとうございます！高尾山の 6 号路で計画してみます。登山届も出しておきます。' },
    ],
  },
  {
    category: 'announcements',
    title: '11 月のオフ会（高尾山）参加者募集',
    as: 'owner',
    body: '11 月 23 日（祝）に高尾山でオフ会を開きます。はじめての方も歓迎です。\n\n集合: 京王線 高尾山口駅 9:00\nコース: 稲荷山コース → 山頂 → 6 号路（約 4 時間）\n\n参加される方はこのスレッドに書き込んでください。',
    replies: [
      { author: 'ハイカー見習い', body: '参加します！はじめてのオフ会なので楽しみです。' },
      { author: 'そらまめ', body: '友人と 2 人で参加します。よろしくお願いします。' },
    ],
    pin: true,
  },
  {
    category: 'announcements',
    title: '掲示板の使い方とお願い（はじめにお読みください）',
    as: 'owner',
    body: 'やまびこへようこそ。この掲示板は、ログインしなくても名前を入れるだけで書き込めます。\n\n- スレッドは内容に合ったカテゴリに立ててください\n- 山小屋や登山道の情報は、いつの情報かを書き添えてください\n- 個人を特定できる情報（住所・電話番号など）は書き込まないでください\n\nガイドラインに反する投稿は、モデレーターが削除することがあります。',
    replies: [],
    pin: true,
    lock: true,
  },
];

const articles = [
  {
    slug: 'autumn-offline-meeting',
    title: '11 月のオフ会（高尾山）の参加者を募集しています',
    excerpt: '11 月 23 日（祝）に高尾山でオフ会を開きます。はじめての方も歓迎です。',
    body: `11 月 23 日（祝）に高尾山でオフ会を開きます。はじめての方も歓迎です。

- 集合: 京王線 高尾山口駅 9:00
- コース: 稲荷山コース → 山頂 → 6 号路（約 4 時間）

参加される方は、掲示板の「お知らせ」にあるスレッドに書き込んでください。`,
  },
  {
    slug: 'forum-opened',
    title: '掲示板をオープンしました',
    excerpt: '山歩きが好きな人のための掲示板「やまびこ」をオープンしました。',
    body: `山歩きが好きな人のための掲示板「やまびこ」をオープンしました。

## カテゴリ

- はじめての山: 山選び・持ち物・歩き方の質問
- 道具・装備: 靴・雨具・ザックの選び方
- 山行記録: 歩いた山の様子や登山道の状況

書き込む前に、投稿ガイドラインをお読みください。`,
  },
];

const pages = [
  {
    slug: 'about',
    title: 'このサイトについて',
    body: `やまびこは、山歩きが好きな人が情報を交換するための掲示板です。運営は有志のメンバーが行っています。

## 運営

- 管理人: サイトとカテゴリの管理
- モデレーター: スレッドの見回り（ピン留め・ロック・削除）

このサイトは AegisCMS のサンプルサイトです。投稿・人物・オフ会は架空のものです。`,
  },
  {
    slug: 'guidelines',
    title: '投稿ガイドライン',
    body: `気持ちよく使える掲示板にするために、次のことをお願いしています。

## 書き込むとき

- スレッドは内容に合ったカテゴリに立てる
- 山小屋や登山道の情報には、いつの情報かを書き添える
- 質問に答えてもらったら、結果を書き込んでスレッドを締めくくる

## 禁止していること

- 個人を特定できる情報（住所・電話番号など）の書き込み
- 他の人への攻撃や、宣伝だけを目的とした書き込み
- 立ち入りが禁止されている場所への登山を勧めること

## モデレーション

ガイドラインに反する書き込みは、モデレーターが削除します。終わった話題のスレッドはロックして、新しい書き込みを止めることがあります。`,
  },
];

// ---------------------------------------------------------------------------
// Markdown（見出し・箇条書き・番号付きリスト・段落だけ）をブロックに変換する
// ---------------------------------------------------------------------------

function toBlocks(markdown) {
  const blocks = [];
  for (const chunk of markdown.trim().split(/\n\s*\n/)) {
    const lines = chunk.split('\n');
    const heading = lines[0].match(/^(#{2,4})\s+(.*)$/);
    if (heading && lines.length === 1) {
      blocks.push({ name: 'core/heading', attributes: { level: heading[1].length }, children: [], text: heading[2] });
    } else if (lines.every((l) => /^- /.test(l))) {
      blocks.push(list(lines.map((l) => l.slice(2)), false));
    } else if (lines.every((l) => /^\d+\.\s/.test(l))) {
      blocks.push(list(lines.map((l) => l.replace(/^\d+\.\s/, '')), true));
    } else {
      blocks.push({ name: 'core/paragraph', attributes: {}, children: [], text: lines.join('') });
    }
  }
  return blocks;
}

function list(items, ordered) {
  return {
    name: 'core/list',
    attributes: { ordered },
    children: items.map((text) => ({ name: 'core/list-item', attributes: {}, children: [], text })),
  };
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

const tokens = {};

// as: ログインして呼ぶユーザー（省略すると未ログイン）
async function api(method, path, body, as) {
  const headers = {};
  if (as) headers.authorization = `Bearer ${tokens[as]}`;
  if (body !== undefined) headers['content-type'] = 'application/json';
  const res = await fetch(BASE + path, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  const data = text ? JSON.parse(text) : null;
  return { status: res.status, data };
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// 掲示板への投稿。連投の制限（[forum].rate_limit_secs）で 429 が返ったら待って投稿し直す
async function post(path, body, as) {
  for (let attempt = 0; ; attempt++) {
    const res = await api('POST', path, body, as);
    if (res.status !== 429 || attempt >= 60) return res;
    await sleep(1000);
  }
}

function check({ status, data }, expected, what) {
  if (status !== expected) {
    throw new Error(`${what}に失敗しました (${status}): ${JSON.stringify(data)}`);
  }
  return data;
}

async function login(username) {
  const res = await fetch(`${BASE}/api/v1/auth/session`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ username, password: PASSWORD }),
  }).catch((e) => {
    throw new Error(`${BASE} に接続できません。サーバーを起動してください (${e.message})`);
  });
  if (!res.ok) {
    throw new Error(`${username} でログインできませんでした (${res.status})。task demo:forum でユーザーを作ったか、--password を確かめてください`);
  }
  tokens[username] = (await res.json()).id;
}

async function createContent(type, record) {
  const path = `/api/v1/contents/${type}/${encodeURIComponent(record.slug)}`;
  if ((await api('GET', path, undefined, 'owner')).status === 200) {
    console.log(`  skip   ${type}/${record.slug}（作成済み）`);
    return;
  }
  const { status, data } = await api('POST', `/api/v1/contents/${type}`, record, 'owner');
  if (status !== 201 && status !== 200) {
    throw new Error(`${type}/${record.slug} を作成できませんでした (${status}): ${JSON.stringify(data)}`);
  }
  console.log(`  create ${type}/${record.slug}`);
}

// ---------------------------------------------------------------------------

async function main() {
  for (const u of USERS) await login(u);
  console.log(`${BASE} に ${USERS.join(' と ')} でログインしました`);

  const forum = await api('GET', '/api/v1/forum/categories');
  if (forum.status !== 200) {
    throw new Error('掲示板が有効になっていません。-c examples/hiking-forum/aegis.toml で起動してください');
  }

  console.log('掲示板のカテゴリ');
  const categoryIds = {};
  for (const [position, c] of categories.entries()) {
    const found = await api('GET', `/api/v1/forum/categories/${c.slug}`);
    if (found.status === 200) {
      console.log(`  skip   ${c.slug}（作成済み）`);
      categoryIds[c.slug] = found.data.category.id;
      continue;
    }
    const data = check(
      await api('POST', '/api/v1/forum/categories', { ...c, position, is_private: false }, 'owner'),
      201,
      `カテゴリ ${c.slug} の作成`,
    );
    console.log(`  create ${c.slug}`);
    categoryIds[c.slug] = data.category.id;
  }

  console.log('スレッドとレス（連投の制限で待ちながら投稿するので、1 分ほどかかります）');
  for (const t of threads) {
    const { data: listed } = await api('GET', `/api/v1/forum/categories/${t.category}/threads?limit=100`);
    if (listed.threads.some((x) => x.title === t.title)) {
      console.log(`  skip   ${t.title}（作成済み）`);
      continue;
    }
    const created = check(
      await post(
        '/api/v1/forum/threads',
        { category_id: categoryIds[t.category], title: t.title, body: t.body, author_name: t.author ?? '' },
        t.as,
      ),
      201,
      `スレッド「${t.title}」の作成`,
    );
    const threadId = created.thread.id;
    console.log(`  create ${t.title}`);

    for (const r of t.replies) {
      check(
        await post(
          '/api/v1/forum/posts',
          { thread_id: threadId, body: r.body, author_name: r.author ?? '', reply_to_number: r.reply_to ?? null },
          r.as,
        ),
        201,
        `「${t.title}」へのレス`,
      );
    }
    if (t.replies.length > 0) console.log(`         レス ${t.replies.length} 件`);

    // ピン留め・ロックはモデレーター（editor）が行う
    if (t.pin) {
      check(await api('POST', `/api/v1/forum/threads/${threadId}/pin`, { pinned: true }, 'moderator'), 200, 'ピン留め');
      console.log('         ピン留め');
    }
    if (t.lock) {
      check(await api('POST', `/api/v1/forum/threads/${threadId}/lock`, { locked: true }, 'moderator'), 200, 'ロック');
      console.log('         ロック');
    }
  }

  console.log('お知らせ（記事）');
  for (const a of [...articles].reverse()) {
    await createContent('article', {
      title: a.title,
      slug: a.slug,
      status: 'published',
      excerpt: a.excerpt,
      blocks: toBlocks(a.body),
    });
  }

  console.log('固定ページ');
  for (const p of pages) {
    await createContent('page', { title: p.title, slug: p.slug, status: 'published', blocks: toBlocks(p.body) });
  }

  console.log(`\n完了: ${BASE}/forum を開いてください（管理画面は ${BASE}/admin/）`);
}

main().catch((e) => {
  console.error(e.message);
  process.exit(1);
});
