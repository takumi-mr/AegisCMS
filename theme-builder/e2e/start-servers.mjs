// Starts what the E2E tests need (Playwright `webServer`): the theme dev server
// (`aegis-server theme dev`) on a copy of the fixture theme, and Theme Builder's Vite dev server
// in front of it. Ports are not the ones of `task dev:theme` / `task dev:theme-builder`.
import { spawn, spawnSync } from 'node:child_process';
import { join } from 'node:path';

import { BUILDER_PORT, DEV_SERVER_PORT, resetTheme, ROOT, THEME_DIR } from './fixture.mjs';

const isWindows = process.platform === 'win32';
const server = join(ROOT, 'target', 'debug', isWindows ? 'aegis-server.exe' : 'aegis-server');

resetTheme();

if (!process.env.E2E_SKIP_BUILD) {
    const build = spawnSync('cargo', ['build', '-p', 'aegis-server'], {
        cwd: ROOT,
        stdio: 'inherit',
    });
    if (build.status !== 0) {
        console.error('[e2e] "cargo build -p aegis-server" failed');
        process.exit(build.status ?? 1);
    }
}

const children = [
    spawn(server, ['theme', 'dev', THEME_DIR, '--port', String(DEV_SERVER_PORT)], {
        cwd: ROOT,
        stdio: 'inherit',
        env: { RUST_LOG: 'warn', ...process.env },
    }),
    spawn('npx', ['vite', '--port', String(BUILDER_PORT), '--strictPort'], {
        cwd: join(ROOT, 'theme-builder'),
        stdio: 'inherit',
        // npx is a .cmd script on Windows
        shell: isWindows,
        env: { ...process.env, THEME_DEV_URL: `http://127.0.0.1:${DEV_SERVER_PORT}` },
    }),
];

const stop = (signal) => children.forEach((child) => child.kill(signal));
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => stop(signal));
// If either one dies, the tests cannot run: stop the other and report it
for (const child of children) {
    child.on('exit', (code) => {
        stop('SIGTERM');
        process.exit(code ?? 0);
    });
}
