// Starts a throwaway core server for the E2E tests (Playwright `webServer`): builds Admin Studio
// and the server, wipes target/e2e/, creates the admin user and serves on port 18080.
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { E2E_PASSWORD, E2E_USERNAME } from './credentials.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const config = 'admin/e2e/aegis.e2e.toml';
const isWindows = process.platform === 'win32';
const server = join(root, 'target', 'debug', isWindows ? 'aegis-server.exe' : 'aegis-server');

const run = (command, args, options = {}) => {
    const result = spawnSync(command, args, {
        cwd: root,
        stdio: 'inherit',
        // npm is a .cmd script on Windows
        shell: isWindows && command === 'npm',
        ...options,
    });
    if (result.status !== 0) {
        console.error(`[e2e] "${command} ${args.join(' ')}" failed`);
        process.exit(result.status ?? 1);
    }
};

const dataDir = join(root, 'target', 'e2e');
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });

if (!process.env.E2E_SKIP_BUILD) {
    run('npm', ['--prefix', 'admin', 'run', 'build']);
    run('cargo', ['build', '-p', 'aegis-server']);
}
run(server, ['-c', config, 'user', 'add', E2E_USERNAME, '--role', 'admin'], {
    env: { ...process.env, AEGIS_USER_PASSWORD: E2E_PASSWORD },
});

// Request logs drown the test output; RUST_LOG=debug brings them back
const child = spawn(server, ['-c', config, 'run'], {
    cwd: root,
    stdio: 'inherit',
    env: { RUST_LOG: 'warn', ...process.env },
});
for (const signal of ['SIGINT', 'SIGTERM']) {
    process.on(signal, () => child.kill(signal));
}
child.on('exit', (code) => process.exit(code ?? 0));
