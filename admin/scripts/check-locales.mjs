// Checks that every language has the same translation keys as English (src/locales/en).
// Plural forms (key_one, key_other, ...) count as one key, since languages have different forms.
import { readFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const localesDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'src', 'locales');
const PLURAL = /_(zero|one|two|few|many|other)$/;

function keys(obj, prefix = '') {
    return Object.entries(obj).flatMap(([key, value]) => {
        const path = prefix ? `${prefix}.${key}` : key;
        if (value && typeof value === 'object') return keys(value, path);
        if (typeof value !== 'string' || value.trim() === '') return [`${path} (empty)`];
        return [path.replace(PLURAL, '')];
    });
}

const load = (lang) =>
    new Set(keys(JSON.parse(readFileSync(join(localesDir, lang, 'translation.json'), 'utf8'))));

const source = load('en');
let failed = false;
for (const lang of readdirSync(localesDir).filter((l) => l !== 'en')) {
    const target = load(lang);
    const missing = [...source].filter((k) => !target.has(k));
    const extra = [...target].filter((k) => !source.has(k));
    for (const k of missing) console.error(`${lang}: missing ${k}`);
    for (const k of extra) console.error(`${lang}: not in en: ${k}`);
    if (missing.length || extra.length) failed = true;
}
for (const k of source) {
    if (k.endsWith('(empty)')) {
        console.error(`en: ${k}`);
        failed = true;
    }
}
if (failed) process.exit(1);
console.log('Translations: all languages have the same keys');
