#!/usr/bin/env node
// サンプルショップ「朝凪珈琲」のデータ（カテゴリ・商品・画像・記事・固定ページ）を API で投入する。
// 既にある slug はスキップするので、何度実行してもよい。依存パッケージは不要（Node.js 18 以上）。
//
//   node examples/coffee-shop/seed.mjs [--url http://127.0.0.1:8080] [--user owner] [--password ...]
//
// パスワードは --password か環境変数 AEGIS_SEED_PASSWORD（既定: sample-password）。

import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : fallback;
}

const BASE = arg('url', 'http://127.0.0.1:8080').replace(/\/$/, '');
const USER = arg('user', 'owner');
const PASSWORD = arg('password', process.env.AEGIS_SEED_PASSWORD ?? 'sample-password');

// ---------------------------------------------------------------------------
// データ
// ---------------------------------------------------------------------------

const categories = [
  { slug: 'beans', title: '珈琲豆', color: '#a4552c', description: 'シングルオリジンとブレンド。200g 袋。' },
  { slug: 'drip-bags', title: 'ドリップバッグ', color: '#3f6b3a', description: 'カップに掛けてお湯を注ぐだけ。' },
  { slug: 'tools', title: '器具・うつわ', color: '#2f5577', description: '毎日のコーヒーに使う道具。' },
  { slug: 'gifts', title: 'ギフト', color: '#6a4c8c', description: '贈りものにどうぞ。' },
];

// price / compare_at_price は円（税込み）。stock: null は在庫を数えない
const products = [
  {
    slug: 'ethiopia-yirgacheffe',
    title: 'エチオピア イルガチェフェ',
    category: 'beans',
    price: 1780,
    stock: 24,
    sku: 'BEAN-ETH-200',
    excerpt: 'ジャスミンのような香りと、レモンティーのような明るい酸味。浅煎り。',
    body: `## 味わい

花のような香りと、紅茶のように軽やかな後味。冷めるにつれて甘さが増していきます。

- 焙煎度: 浅煎り
- 産地: エチオピア ゲデオ地区
- 精製: ウォッシュト
- 内容量: 200g（豆のまま）

## おすすめの淹れ方

豆 15g に 92℃ のお湯 240ml。ペーパードリップで、3 分を目安に。`,
  },
  {
    slug: 'guatemala-antigua',
    title: 'グアテマラ アンティグア',
    category: 'beans',
    price: 1580,
    stock: 30,
    sku: 'BEAN-GTM-200',
    excerpt: 'ミルクチョコレートとナッツの甘さ。毎日飲みたいバランスの中煎り。',
    body: `## 味わい

なめらかな口当たりに、チョコレートとローストナッツの甘い余韻。ミルクとの相性も良い一杯です。

- 焙煎度: 中煎り
- 産地: グアテマラ アンティグア
- 精製: ウォッシュト
- 内容量: 200g（豆のまま）`,
  },
  {
    slug: 'house-blend-asanagi',
    title: 'ハウスブレンド「朝凪」',
    category: 'beans',
    price: 1380,
    compare_at_price: 1580,
    stock: 50,
    sku: 'BEAN-BLD-200',
    excerpt: '店名を冠したいちばん人気のブレンド。今月はお試し価格です。',
    body: `## 味わい

ブラジルとグアテマラをベースに、エチオピアで華やかさを添えました。苦味と甘さのちょうど真ん中。

- 焙煎度: 中深煎り
- 内容量: 200g（豆のまま）

## 今月のお試し価格

通常 1,580円のところ、今月は 1,380円でお届けします。`,
  },
  {
    slug: 'decaf-colombia',
    title: 'デカフェ コロンビア',
    category: 'beans',
    price: 1680,
    stock: 0,
    sku: 'BEAN-DCF-200',
    excerpt: 'カフェインを 99.9% 取り除いても、甘さとコクはそのまま。',
    body: `## 味わい

マウンテンウォーター製法でカフェインを除いたコロンビア。夜のひとときにも。

- 焙煎度: 中深煎り
- 内容量: 200g（豆のまま）

ただいま入荷待ちです。次回の焙煎は来月上旬の予定です。`,
  },
  {
    slug: 'drip-bag-assort',
    title: 'ドリップバッグ 10 個アソート',
    category: 'drip-bags',
    price: 1500,
    stock: 3,
    sku: 'DRIP-ASSORT-10',
    excerpt: '定番 5 種を 2 個ずつ。はじめての方やお試しに。',
    body: `## セット内容

エチオピア、グアテマラ、朝凪ブレンド、デカフェ コロンビア、季節のブレンドを 2 個ずつ。

- 1 袋 12g（1 杯分）
- 賞味期限: 製造から 6 か月`,
  },
  {
    slug: 'arita-dripper',
    title: '有田焼ドリッパー',
    category: 'tools',
    price: 3300,
    stock: 12,
    sku: 'TOOL-DRP-ARITA',
    excerpt: '白磁に藍の線。お湯の通りが均一になるリブ付き。1〜4 杯用。',
    body: `## 特長

佐賀・有田の窯元と作った磁器のドリッパー。熱を保ちやすく、抽出の温度が安定します。

- サイズ: 1〜4 杯用（台形 4 号のフィルター）
- 素材: 磁器
- 食洗機: 使用できます`,
  },
  {
    slug: 'hand-grinder',
    title: '手挽きコーヒーミル',
    category: 'tools',
    price: 8800,
    stock: 5,
    sku: 'TOOL-GRD-HAND',
    excerpt: 'セラミックの臼で、極細挽きからフレンチプレス用の粗挽きまで。',
    body: `## 特長

木の胴とセラミックの臼。挽き目はダイヤルで 12 段階に調節できます。

- 容量: 30g
- 素材: 天然木・ステンレス・セラミック
- 分解して水洗いできます`,
  },
  {
    slug: 'shigaraki-mug',
    title: '信楽焼マグカップ',
    category: 'tools',
    price: 2400,
    stock: null,
    sku: 'TOOL-MUG-SHGRK',
    excerpt: '土のぬくもりが手に伝わる、ぽってりとしたマグ。一つずつ手作り。',
    body: `## 特長

滋賀・信楽の土を使った陶器のマグ。釉薬の流れや鉄粉の表情は一つずつ異なります。

- 容量: 300ml
- 電子レンジ: 使用できます`,
  },
  {
    slug: 'seasonal-gift-set',
    title: '季節のギフトセット',
    category: 'gifts',
    price: 4950,
    stock: 20,
    sku: 'GIFT-SEASON',
    excerpt: '珈琲豆 2 種とドリップバッグ 5 個を化粧箱に。のし・メッセージカード対応。',
    body: `## セット内容

- 季節のシングルオリジン 200g
- ハウスブレンド「朝凪」200g
- ドリップバッグ 5 個

化粧箱でお届けします。のし・メッセージカードをご希望の場合は、注文確認メールへの返信でお知らせください。`,
  },
];

const articles = [
  {
    slug: 'how-to-brew-pour-over',
    title: 'はじめてのハンドドリップ',
    excerpt: '道具は 4 つだけ。豆の量・お湯の温度・時間の 3 つを決めれば、毎日同じ味で淹れられます。',
    body: `道具はドリッパー、ペーパーフィルター、ケトル、そしてスケール。豆の量・お湯の温度・時間の 3 つを決めれば、毎日同じ味で淹れられます。

## 基本のレシピ（1 杯分）

- 豆: 15g（中細挽き）
- お湯: 92℃ を 240ml
- 時間: 3 分

## 手順

1 投目は 30ml ほど注いで 30 秒蒸らします。粉がふくらんだら、中心から「の」の字を描くように 3 回に分けて注ぎます。`,
  },
  {
    slug: 'roasting-schedule',
    title: '焙煎のスケジュールについて',
    excerpt: '月・水・金に焙煎しています。ご注文から発送までの流れをご紹介します。',
    body: `朝凪珈琲では、月・水・金の朝に焙煎しています。ご入金を確認したご注文は、次の焙煎日に焼いて翌日までに発送します。

## 届いた豆の保存

袋のチャックを閉めて、直射日光の当たらない涼しい場所へ。2 週間を過ぎる場合は冷凍保存がおすすめです。`,
  },
  {
    slug: 'autumn-lineup',
    title: '秋のラインナップが揃いました',
    excerpt: 'グアテマラの新豆と、ギフトセットの販売を始めました。',
    body: `朝晩が涼しくなってきました。今月からグアテマラ アンティグアの新豆と、季節のギフトセットの販売を始めています。

ハウスブレンド「朝凪」は今月いっぱいお試し価格です。はじめての方は、カートでクーポン WELCOME10 を入力すると 10% オフになります。`,
  },
];

const pages = [
  {
    slug: 'about',
    title: '私たちについて',
    body: `朝凪珈琲は、海の近くの小さな焙煎所です。5kg の焙煎機で、ご注文をいただいてから豆を焼いています。

## 大切にしていること

- 生産者がわかる豆だけを仕入れる
- 焙煎から 3 日以内にお届けする
- 毎日飲んでも飽きない味に仕上げる

このサイトは AegisCMS のサンプルショップです。商品・注文は架空のものです。`,
  },
  {
    slug: 'shipping',
    title: '配送・お支払いについて',
    body: `## 送料

全国一律 650円。5,000円（割引後の商品合計）以上のご注文は送料無料です。

## お支払い

銀行振込のみです。注文確認ページに表示される口座へ、3 日以内にお振り込みください。期限を過ぎたご注文は自動で取り消します。

## クーポン

- WELCOME10: はじめての方は 10% オフ
- COFFEE300: 300円引き`,
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

let token;

async function api(method, path, body) {
  const headers = { authorization: `Bearer ${token}` };
  let payload;
  if (body instanceof FormData) {
    payload = body;
  } else if (body !== undefined) {
    headers['content-type'] = 'application/json';
    payload = JSON.stringify(body);
  }
  const res = await fetch(BASE + path, { method, headers, body: payload });
  const text = await res.text();
  const data = text ? JSON.parse(text) : null;
  return { status: res.status, data };
}

async function exists(type, slug) {
  const { status } = await api('GET', `/api/v1/contents/${type}/${encodeURIComponent(slug)}`);
  return status === 200;
}

async function create(type, record) {
  if (await exists(type, record.slug)) {
    console.log(`  skip   ${type}/${record.slug}（作成済み）`);
    const { data } = await api('GET', `/api/v1/contents/${type}/${encodeURIComponent(record.slug)}`);
    return data;
  }
  const { status, data } = await api('POST', `/api/v1/contents/${type}`, record);
  if (status !== 201 && status !== 200) {
    throw new Error(`${type}/${record.slug} を作成できませんでした (${status}): ${JSON.stringify(data)}`);
  }
  console.log(`  create ${type}/${record.slug}`);
  return data;
}

async function uploadImage(slug) {
  const bytes = await readFile(join(here, 'images', `${slug}.png`));
  const form = new FormData();
  form.append('key', `sample-shop/${slug}.png`);
  form.append('file', new Blob([bytes], { type: 'image/png' }), `${slug}.png`);
  const { status, data } = await api('POST', '/api/v1/storage/upload', form);
  if (status !== 201) {
    throw new Error(`画像 ${slug}.png をアップロードできませんでした (${status}): ${JSON.stringify(data)}`);
  }
  return data.url;
}

// ---------------------------------------------------------------------------

async function main() {
  const login = await fetch(`${BASE}/api/v1/auth/session`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ username: USER, password: PASSWORD }),
  }).catch((e) => {
    throw new Error(`${BASE} に接続できません。サーバーを起動してください (${e.message})`);
  });
  if (!login.ok) {
    throw new Error(`ログインできませんでした (${login.status})。--user / --password を確かめてください`);
  }
  token = (await login.json()).id;
  console.log(`${BASE} に ${USER} でログインしました`);

  console.log('カテゴリ');
  const categoryIds = {};
  for (const c of categories) {
    const record = await create('category', { ...c, status: 'published' });
    categoryIds[c.slug] = record.id;
  }

  // 一覧は新しい順なので、先頭に出したい商品を最後に作る
  console.log('商品');
  for (const p of [...products].reverse()) {
    if (await exists('product', p.slug)) {
      console.log(`  skip   product/${p.slug}（作成済み）`);
      continue;
    }
    const image_url = await uploadImage(p.slug);
    await create('product', {
      title: p.title,
      slug: p.slug,
      status: 'published',
      price: p.price,
      compare_at_price: p.compare_at_price ?? null,
      stock: p.stock,
      sku: p.sku,
      excerpt: p.excerpt,
      image_url,
      category_id: categoryIds[p.category],
      blocks: toBlocks(p.body),
    });
  }

  console.log('記事');
  for (const a of articles) {
    await create('article', {
      title: a.title,
      slug: a.slug,
      status: 'published',
      excerpt: a.excerpt,
      blocks: toBlocks(a.body),
    });
  }

  console.log('固定ページ');
  for (const p of pages) {
    await create('page', { title: p.title, slug: p.slug, status: 'published', blocks: toBlocks(p.body) });
  }

  console.log(`\n完了: ${BASE}/ を開いてください（管理画面は ${BASE}/admin/）`);
}

main().catch((e) => {
  console.error(e.message);
  process.exit(1);
});
