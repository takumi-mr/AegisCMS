#!/usr/bin/env node
/**
 * scripts/new-plugin.mjs
 *
 * AegisCMS プラグインのひな形を生成するスクリプト。
 * 独立クレート化・相対パス調整・名前置換を自動で行います。
 *
 * 使い方:
 *   node scripts/new-plugin.mjs <plugin-name> [--type normal|block] [--lang rust|assemblyscript|tinygo]
 *   task plugin:new -- <plugin-name> [--type block] [--lang as|tinygo]
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');

// 引数のパース
const args = process.argv.slice(2);
let pluginName = '';
let pluginType = 'normal'; // 'normal' | 'block'
let lang = 'rust'; // 'rust' | 'assemblyscript' | 'tinygo'
let force = false;

for (let i = 0; i < args.length; i++) {
  const arg = args[i];
  if (arg === '--help' || arg === '-h') {
    printHelp();
    process.exit(0);
  } else if (arg === '--type' && i + 1 < args.length) {
    pluginType = args[++i];
  } else if (arg === '--block') {
    pluginType = 'block';
  } else if ((arg === '--lang' || arg === '-l') && i + 1 < args.length) {
    const l = args[++i].toLowerCase();
    if (['as', 'assemblyscript', 'ts'].includes(l)) lang = 'assemblyscript';
    else if (['go', 'tinygo'].includes(l)) lang = 'tinygo';
    else lang = 'rust';
  } else if (arg === '--force' || arg === '-f') {
    force = true;
  } else if (!arg.startsWith('-') && !pluginName) {
    pluginName = arg;
  }
}

function printHelp() {
  console.log(`
AegisCMS Plugin Generator

Usage:
  task plugin:new -- <plugin-name> [options]
  node scripts/new-plugin.mjs <plugin-name> [options]

Arguments:
  <plugin-name>              Name of the plugin (e.g. my-plugin, custom-blocks)
                             Must contain only lowercase letters, numbers, and hyphens.

Options:
  --type <normal|block>      Plugin type ('normal' or 'block'). Default: normal
  --block                    Shorthand for '--type block' (Rust block template)
  --lang <rust|as|tinygo>    Programming language. Default: rust
  --force, -f                Overwrite target directory if it already exists
  --help, -h                 Show this help message

Examples:
  task plugin:new -- my-plugin
  task plugin:new -- my-callout --type block
  task plugin:new -- my-as-plugin --lang as
  task plugin:new -- my-go-plugin --lang tinygo
`);
}

if (!pluginName) {
  console.error('Error: Plugin name is required.');
  printHelp();
  process.exit(1);
}

// バリデーション: 英小文字・数字・ハイフン
if (!/^[a-z0-9]+(-[a-z0-9]+)*$/.test(pluginName)) {
  console.error(
    `Error: Invalid plugin name "${pluginName}".\n` +
      'Plugin name must be lowercase alphanumeric characters and hyphens (e.g., "my-plugin", "site-stats").'
  );
  process.exit(1);
}

// テンプレートの決定
let templateDir = '';
if (lang === 'assemblyscript') {
  templateDir = path.join(rootDir, 'sdk', 'templates', 'assemblyscript');
} else if (lang === 'tinygo') {
  templateDir = path.join(rootDir, 'sdk', 'templates', 'tinygo');
} else if (pluginType === 'block') {
  templateDir = path.join(rootDir, 'sdk', 'templates', 'rust-block');
} else {
  templateDir = path.join(rootDir, 'sdk', 'templates', 'rust');
}

if (!fs.existsSync(templateDir)) {
  console.error(`Error: Template directory not found: ${templateDir}`);
  process.exit(1);
}

const targetDir = path.join(rootDir, 'plugins', pluginName);

if (fs.existsSync(targetDir)) {
  if (!force) {
    console.error(
      `Error: Target directory already exists: ${path.relative(rootDir, targetDir)}\n` +
        'Use --force to overwrite.'
    );
    process.exit(1);
  }
  fs.rmSync(targetDir, { recursive: true, force: true });
}

fs.mkdirSync(targetDir, { recursive: true });

// コピー対象外のファイル/ディレクトリ
const ignorePatterns = [
  'node_modules',
  'target',
  '.git',
  'plugin.wasm',
  'package-lock.json',
];

function copyRecursive(src, dest) {
  const entries = fs.readdirSync(src, { withFileTypes: true });
  for (const entry of entries) {
    if (ignorePatterns.includes(entry.name)) continue;

    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);

    if (entry.isDirectory()) {
      fs.mkdirSync(destPath, { recursive: true });
      copyRecursive(srcPath, destPath);
    } else {
      fs.copyFileSync(srcPath, destPath);
    }
  }
}

copyRecursive(templateDir, targetDir);

// 1. plugin.manifest.json の置換
const manifestPath = path.join(targetDir, 'plugin.manifest.json');
if (fs.existsSync(manifestPath)) {
  const content = fs.readFileSync(manifestPath, 'utf-8');
  try {
    const manifest = JSON.parse(content);
    manifest.name = pluginName;
    fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n', 'utf-8');
  } catch (err) {
    // パース失敗時は単純置換
    const updated = content.replace(/"name":\s*"[^"]+"/, `"name": "${pluginName}"`);
    fs.writeFileSync(manifestPath, updated, 'utf-8');
  }
}

// 2. 言語ごとの設定調整
if (lang === 'rust') {
  const cargoPath = path.join(targetDir, 'Cargo.toml');
  if (fs.existsSync(cargoPath)) {
    let content = fs.readFileSync(cargoPath, 'utf-8');
    // パッケージ名の置換
    content = content.replace(/name = "[^"]+"/, `name = "${pluginName}"`);
    // 相対パスの深さ調整: sdk/templates (3段 ../../../) から plugins/ (2段 ../../)
    content = content.replace(
      /path = "\.\.\/\.\.\/\.\.\/crates\/aegis-plugin-sdk"/g,
      'path = "../../crates/aegis-plugin-sdk"'
    );
    // 独立クレート化: ルートの workspace member でないため、空の [workspace] を追加して競合を防止
    if (!content.includes('[workspace]')) {
      content = content.trimEnd() + '\n\n[workspace]\n';
    }
    fs.writeFileSync(cargoPath, content, 'utf-8');
  }
} else if (lang === 'assemblyscript') {
  const pkgPath = path.join(targetDir, 'package.json');
  if (fs.existsSync(pkgPath)) {
    let content = fs.readFileSync(pkgPath, 'utf-8');
    try {
      const pkg = JSON.parse(content);
      pkg.name = pluginName;
      fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n', 'utf-8');
    } catch {
      content = content.replace(/"name":\s*"[^"]+"/, `"name": "${pluginName}"`);
      fs.writeFileSync(pkgPath, content, 'utf-8');
    }
  }
}

console.log(`\n🎉 Successfully created plugin: ${pluginName}`);
console.log(`   Location: ${path.relative(rootDir, targetDir)}`);
console.log(`   Template: ${path.basename(templateDir)} (${lang})\n`);

console.log('Next steps:');
if (lang === 'rust') {
  console.log(`  1. Build the WebAssembly binary:`);
  console.log(`     cd plugins/${pluginName}`);
  console.log(`     cargo build --target wasm32-unknown-unknown --release`);
  console.log(`     copy target\\wasm32-unknown-unknown\\release\\${pluginName.replace(/-/g, '_')}.wasm plugin.wasm   # Windows`);
  console.log(`     cp target/wasm32-unknown-unknown/release/${pluginName.replace(/-/g, '_')}.wasm plugin.wasm       # Linux/macOS`);
} else if (lang === 'assemblyscript') {
  console.log(`  1. Install dependencies and build:`);
  console.log(`     cd plugins/${pluginName}`);
  console.log(`     npm install`);
  console.log(`     npm run build`);
} else if (lang === 'tinygo') {
  console.log(`  1. Build with TinyGo:`);
  console.log(`     cd plugins/${pluginName}`);
  console.log(`     tinygo build -target=wasm-unknown -opt=z -no-debug -o plugin.wasm main.go`);
}

console.log(`\n  2. Test and develop with live-reload:`);
console.log(`     task dev:plugin -- plugins/${pluginName}\n`);

