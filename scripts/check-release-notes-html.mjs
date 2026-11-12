// PR のタイトル・本文・コミットメッセージに、リリース PR を壊す生の HTML が入っていないか検査する。
//
// release-please はリリース PR の本文（コミットメッセージから作る）を HTML として読む。
// `<script>` / `<style>` のような要素が本文に生で入ると、その先の見出し（`<details>`）が読めなくなり、
// リリース PR をマージしてもタグと GitHub Release が作られず、以降のリリース PR も作られなくなる
// （docs/versioning.md の 3）。バッククォートで囲んでも防げないので、言葉で書く。
//
// 使い方: PR_TITLE / PR_BODY を環境変数に、コミットメッセージを標準入力に渡す
//   git log origin/main..HEAD --format=%B | PR_TITLE=... PR_BODY=... node scripts/check-release-notes-html.mjs
import { readFileSync } from 'node:fs';

/** release-please の本文の読み取りを壊す HTML（script・style の要素と、閉じていないかもしれないコメント） */
const RISKY_HTML = /<\/?(?:script|style)\b|<!--/i;

/** 危険な箇所のある行を返す */
const findRiskyLines = (text) =>
  text
    .split(/\r?\n/)
    .map((line, index) => ({ line: index + 1, text: line }))
    .filter(({ text }) => RISKY_HTML.test(text));

const main = () => {
  const sources = {
    'PR title': process.env.PR_TITLE ?? '',
    'PR body': process.env.PR_BODY ?? '',
    'commit messages': readFileSync(0, 'utf8'),
  };
  let failed = false;
  for (const [name, text] of Object.entries(sources)) {
    for (const { line, text: content } of findRiskyLines(text)) {
      failed = true;
      console.error(`${name}, line ${line}: ${content.trim().slice(0, 120)}`);
    }
  }
  if (failed) {
    console.error(
      '\nA raw <script>, <style> or <!-- in a PR or commit message breaks the release PR that ' +
        'release-please builds from it (no tag or GitHub Release is created). Backticks do not help: ' +
        'describe it in words instead (e.g. "a script element"). See docs/versioning.md.'
    );
    process.exit(1);
  }
  console.log('No risky HTML in the PR title, body or commit messages.');
};

main();
