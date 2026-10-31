#!/usr/bin/env node
/**
 * AegisCMS Database Benchmark Comparison Script
 *
 * Compares HTTP throughput (req/s) and latency distribution (p50, p90, p99)
 * across SQLite (WAL mode), PostgreSQL 16 (Docker), and MySQL 8.0 (Docker).
 *
 * Usage:
 *   node scripts/compare-dbs.mjs
 */

import http from 'node:http';
import { spawn } from 'node:child_process';
import { performance } from 'node:perf_hooks';

const DBS = [
  { name: 'SQLite (WAL)', config: 'config/bench-sqlite.toml' },
  { name: 'PostgreSQL 16 (Docker)', config: 'config/bench-postgres.toml' },
  { name: 'MySQL 8.0 (Docker)', config: 'config/bench-mysql.toml' },
];

const BIN_PATH = process.platform === 'win32' ? '.\\target\\release\\aegis-server.exe' : './target/release/aegis-server';

function httpRequest(options, body) {
  return new Promise((resolve, reject) => {
    const req = http.request(options, (res) => {
      let data = '';
      res.on('data', (chunk) => (data += chunk));
      res.on('end', () => resolve({ status: res.statusCode, data }));
    });
    req.on('error', reject);
    if (body) req.write(body);
    req.end();
  });
}

async function waitForServer(port = 8080, maxRetries = 40) {
  for (let i = 0; i < maxRetries; i++) {
    try {
      const res = await httpRequest({
        hostname: '127.0.0.1',
        port,
        path: '/health',
        method: 'GET',
        timeout: 1000,
      });
      if (res.status === 200) return true;
    } catch {
      // wait
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`Server did not start on port ${port}`);
}

async function runBenchEndpoint({ path, method = 'GET', headers = {}, body = null, concurrency = 50, durationSec = 4 }) {
  const agent = new http.Agent({ keepAlive: true, maxSockets: concurrency * 2 });
  const latencies = [];
  const statusCounts = {};
  let totalRequests = 0;

  const reqHeaders = {
    Connection: 'keep-alive',
    'User-Agent': 'AegisCMS-Bench/1.0',
    ...headers,
  };
  if (body) {
    reqHeaders['Content-Length'] = Buffer.byteLength(body);
    reqHeaders['Content-Type'] = 'application/json';
  }

  const reqOptions = {
    hostname: '127.0.0.1',
    port: 8080,
    path,
    method,
    headers: reqHeaders,
    agent,
  };

  const startTime = performance.now();
  const endTime = startTime + durationSec * 1000;
  let active = 0;
  let stopped = false;

  return new Promise((resolve) => {
    function fireNext() {
      if (stopped || performance.now() >= endTime) {
        stopped = true;
        if (active === 0) finish();
        return;
      }
      active++;
      const reqStart = performance.now();
      const req = http.request(reqOptions, (res) => {
        res.on('data', () => {});
        res.on('end', () => {
          latencies.push(performance.now() - reqStart);
          totalRequests++;
          statusCounts[res.statusCode] = (statusCounts[res.statusCode] || 0) + 1;
          active--;
          fireNext();
        });
      });
      req.on('error', () => {
        active--;
        fireNext();
      });
      req.setTimeout(5000, () => req.destroy());
      if (body) req.write(body);
      req.end();
    }

    function finish() {
      agent.destroy();
      const elapsed = (performance.now() - startTime) / 1000;
      latencies.sort((a, b) => a - b);
      const p50 = latencies[Math.floor(latencies.length * 0.5)] || 0;
      const p90 = latencies[Math.floor(latencies.length * 0.9)] || 0;
      const p99 = latencies[Math.floor(latencies.length * 0.99)] || 0;
      const rps = totalRequests / elapsed;
      resolve({ totalRequests, elapsed, rps, p50, p90, p99, statusCounts });
    }

    for (let i = 0; i < concurrency; i++) fireNext();
  });
}

async function main() {
  console.log('=== AegisCMS Database Benchmark Comparison ===\n');
  const results = {};

  for (const db of DBS) {
    console.log(`\n>>> Starting server with ${db.name} (${db.config})...`);
    const serverProc = spawn(BIN_PATH, ['--config', db.config, 'run'], {
      stdio: 'ignore',
      detached: false,
    });

    try {
      await waitForServer(8080);
      console.log(`Server online. Running benchmarks...`);

      // 1. Public Top Page (Read-heavy, minijinja render, no audit sync insert)
      process.stdout.write('  [1/3] GET / (Public index, c=50, 4s)... ');
      const rIndex = await runBenchEndpoint({ path: '/', concurrency: 50, durationSec: 4 });
      console.log(`${rIndex.rps.toFixed(0)} req/s (p50: ${rIndex.p50.toFixed(1)}ms, p99: ${rIndex.p99.toFixed(1)}ms)`);

      // 2. Unauthenticated API (401 + AUDIT LOG SYNC INSERT into _aegis_audit_logs)
      process.stdout.write('  [2/3] GET /api/v1/contents/article (401 + Audit Sync INSERT, c=50, 4s)... ');
      const rAudit = await runBenchEndpoint({ path: '/api/v1/contents/article', concurrency: 50, durationSec: 4 });
      console.log(`${rAudit.rps.toFixed(0)} req/s (p50: ${rAudit.p50.toFixed(1)}ms, p99: ${rAudit.p99.toFixed(1)}ms)`);

      // 3. Login POST /api/v1/auth/session (Auth check + Session create + Audit Sync INSERT, c=20, 3s)
      process.stdout.write('  [3/3] POST /api/v1/auth/session (Login + Audit Sync INSERT, c=20, 3s)... ');
      const loginBody = JSON.stringify({ username: 'admin', password: 'password123' });
      const rLogin = await runBenchEndpoint({
        path: '/api/v1/auth/session',
        method: 'POST',
        body: loginBody,
        concurrency: 20,
        durationSec: 3,
      });
      console.log(`${rLogin.rps.toFixed(0)} req/s (p50: ${rLogin.p50.toFixed(1)}ms, p99: ${rLogin.p99.toFixed(1)}ms)`);

      results[db.name] = { index: rIndex, audit: rAudit, login: rLogin };
    } finally {
      console.log(`Stopping server for ${db.name}...`);
      serverProc.kill('SIGTERM');
      await new Promise((r) => setTimeout(r, 1000));
      try {
        process.kill(serverProc.pid, 'SIGKILL');
      } catch {}
      await new Promise((r) => setTimeout(r, 1000));
    }
  }

  console.log('\n========================================');
  console.log('RESULTS SUMMARY TABLE');
  console.log('========================================\n');
  console.log('| DB Engine | GET / (Public) | GET /api/... (Audit 401) | POST /api/.../session (Login) |');
  console.log('| :--- | :---: | :---: | :---: |');
  for (const [name, res] of Object.entries(results)) {
    const idx = `${res.index.rps.toFixed(0)} req/s (p50: ${res.index.p50.toFixed(1)}ms)`;
    const aud = `${res.audit.rps.toFixed(0)} req/s (p50: ${res.audit.p50.toFixed(1)}ms)`;
    const log = `${res.login.rps.toFixed(0)} req/s (p50: ${res.login.p50.toFixed(1)}ms)`;
    console.log(`| ${name} | ${idx} | ${aud} | ${log} |`);
  }
}

main().catch((err) => {
  console.error('Benchmark failed:', err);
  process.exit(1);
});
