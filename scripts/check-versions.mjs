// リリース対象のバージョン表記が .release-please-manifest.json と食い違っていないか検査する。
//
// - コア（"."）: Cargo workspace、Admin Studio / Theme Builder / Plugin Builder の package.json、
//   SDK マクロ・Rust テンプレートの aegis-plugin-sdk へのバージョン要求
// - sdk/assemblyscript: package.json
// - plugins/official-*: Cargo.toml の version と plugin.manifest.json の version
//   （manifest の abi_version / min_aegis_version がホストで扱えるかも確かめる）
//
// 使い方: node scripts/check-versions.mjs
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(fileURLToPath(import.meta.url), '..', '..');
const errors = [];

const read = (path) => readFileSync(join(root, path), 'utf8');
const readJson = (path) => JSON.parse(read(path));
const fail = (message) => errors.push(message);

const expect = (label, actual, expected) => {
  if (actual !== expected) fail(`${label}: expected ${expected}, found ${actual ?? '(missing)'}`);
};

/** `[header]` から次の `[` までの本文 */
const tomlSection = (text, header) => {
  const start = text.search(new RegExp(`^\\[${header.replace(/\./g, '\\.')}\\]\\s*$`, 'm'));
  if (start < 0) return '';
  const rest = text.slice(start).split('\n').slice(1).join('\n');
  const end = rest.search(/^\[/m);
  return end < 0 ? rest : rest.slice(0, end);
};

const tomlString = (section, key) =>
  section.match(new RegExp(`^${key}\\s*=\\s*"([^"]*)"`, 'm'))?.[1];

/** `name = { ..., version = "x" }` の x */
const dependencyVersion = (text, name) => {
  const section = tomlSection(text, 'dependencies');
  const line = section.split('\n').find((l) => l.startsWith(`${name} `) || l.startsWith(`${name}=`));
  return line?.match(/version\s*=\s*"([^"]*)"/)?.[1];
};

const parseSemver = (v) => v.split(/[-+]/)[0].split('.').map(Number);
const semverLte = (a, b) => {
  const [x, y] = [parseSemver(a), parseSemver(b)];
  for (let i = 0; i < 3; i++) {
    if (x[i] !== y[i]) return x[i] < y[i];
  }
  return true;
};

const versions = readJson('.release-please-manifest.json');
const config = readJson('release-please-config.json');

// 設定とマニフェスト、ディスク上のプラグインの突き合わせ
for (const path of Object.keys(config.packages)) {
  if (!(path in versions)) fail(`${path}: missing from .release-please-manifest.json`);
}
for (const path of Object.keys(versions)) {
  if (!(path in config.packages)) fail(`${path}: missing from release-please-config.json`);
}
for (const dir of readdirSync(join(root, 'plugins'), { withFileTypes: true })) {
  if (dir.isDirectory() && dir.name.startsWith('official-') && !(`plugins/${dir.name}` in versions)) {
    fail(`plugins/${dir.name}: not registered in release-please-config.json`);
  }
}
for (const [path, version] of Object.entries(versions)) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) fail(`${path}: "${version}" is not a plain SemVer version`);
}

// コア
const core = versions['.'];
const workspace = read('Cargo.toml');
expect('Cargo.toml [workspace.package]', tomlString(tomlSection(workspace, 'workspace.package'), 'version'), core);
for (const entry of readdirSync(join(root, 'crates'))) {
  const manifest = join('crates', entry, 'Cargo.toml');
  if (!existsSync(join(root, manifest))) continue;
  const section = tomlSection(read(manifest), 'package');
  if (!/^version\.workspace\s*=\s*true/m.test(section)) {
    fail(`${manifest}: must use "version.workspace = true"`);
  }
}
expect(
  'crates/aegis-plugin-sdk/Cargo.toml aegis-plugin-macros',
  dependencyVersion(read('crates/aegis-plugin-sdk/Cargo.toml'), 'aegis-plugin-macros'),
  core,
);
expect(
  'sdk/templates/rust/Cargo.toml aegis-plugin-sdk',
  dependencyVersion(read('sdk/templates/rust/Cargo.toml'), 'aegis-plugin-sdk'),
  core,
);
for (const app of ['admin', 'theme-builder', 'plugin-builder']) {
  expect(`${app}/package.json`, readJson(`${app}/package.json`).version, core);
  const lock = readJson(`${app}/package-lock.json`);
  expect(`${app}/package-lock.json`, lock.version, core);
  expect(`${app}/package-lock.json packages[""]`, lock.packages?.['']?.version, core);
}

// SDK
const asSdk = versions['sdk/assemblyscript'];
expect('sdk/assemblyscript/package.json', readJson('sdk/assemblyscript/package.json').version, asSdk);
const asLock = readJson('sdk/assemblyscript/package-lock.json');
expect('sdk/assemblyscript/package-lock.json', asLock.version, asSdk);
expect('sdk/assemblyscript/package-lock.json packages[""]', asLock.packages?.['']?.version, asSdk);

// ホストが扱う ABI バージョン（aegis-core の定数）
const abiMatch = read('crates/aegis-core/src/package.rs').match(
  /pub const PLUGIN_ABI_VERSION: u32 = (\d+);/,
);
const hostAbi = abiMatch ? Number(abiMatch[1]) : undefined;
if (hostAbi === undefined) fail('crates/aegis-core/src/package.rs: PLUGIN_ABI_VERSION not found');

// プラグイン（公式プラグインとテンプレート・サンプルの manifest）
const manifestFiles = [
  ...Object.keys(versions).filter((p) => p.startsWith('plugins/')),
  'sdk/templates/rust',
  'sdk/templates/assemblyscript',
  'sdk/templates/tinygo',
  'sdk/examples/assemblyscript-hooks',
  'sdk/examples/slack-notifier',
  'sdk/tinygo/examples/hooks',
].map((dir) => `${dir}/plugin.manifest.json`);
for (const file of manifestFiles) {
  const manifest = readJson(file);
  if (!Number.isInteger(manifest.abi_version)) {
    fail(`${file}: abi_version is required`);
  } else if (hostAbi !== undefined && (manifest.abi_version < 1 || manifest.abi_version > hostAbi)) {
    fail(`${file}: abi_version ${manifest.abi_version} is not supported by the host (${hostAbi})`);
  }
  if (manifest.min_aegis_version && !semverLte(manifest.min_aegis_version, core)) {
    fail(`${file}: min_aegis_version ${manifest.min_aegis_version} is newer than the core ${core}`);
  }
}
for (const [path, version] of Object.entries(versions)) {
  if (!path.startsWith('plugins/')) continue;
  expect(`${path}/plugin.manifest.json`, readJson(`${path}/plugin.manifest.json`).version, version);
  expect(
    `${path}/Cargo.toml`,
    tomlString(tomlSection(read(`${path}/Cargo.toml`), 'package'), 'version'),
    version,
  );
}

if (errors.length > 0) {
  console.error(`Version check failed (${errors.length}):`);
  for (const message of errors) console.error(`  - ${message}`);
  process.exit(1);
}
console.log(`Version check passed (core ${core}, ${Object.keys(versions).length} packages).`);
