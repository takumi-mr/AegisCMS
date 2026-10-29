// The theme the E2E dev server serves. It is a copy of e2e/fixtures/e2e-theme under target/, so the
// tests can change it freely; `resetTheme()` puts the original back before each test.
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
export const FIXTURE_DIR = resolve(dirname(fileURLToPath(import.meta.url)), 'fixtures/e2e-theme');
export const THEMES_DIR = join(ROOT, 'target', 'e2e-themes');
export const THEME_DIR = join(THEMES_DIR, 'e2e-theme');
export const DEV_SERVER_PORT = 18090;
export const BUILDER_PORT = 15174;

/** Files whose line breaks are CRLF in the served copy (git may normalise the fixture's own) */
const CRLF_FILES = ['templates/article.html'];

export function resetTheme() {
    rmSync(THEME_DIR, { recursive: true, force: true });
    mkdirSync(THEMES_DIR, { recursive: true });
    cpSync(FIXTURE_DIR, THEME_DIR, { recursive: true });
    for (const file of CRLF_FILES) {
        const path = join(THEME_DIR, file);
        const text = readFileSync(path, 'utf8').replace(/\r\n/g, '\n').replace(/\n/g, '\r\n');
        writeFileSync(path, text);
    }
}
