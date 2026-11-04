#!/usr/bin/env node
/**
 * AegisCMS Database Benchmark Comparison Script (Pure Performance without Audit DB Writes)
 *
 * Compares HTTP throughput (req/s) and latency distribution (p50, p90, p99)
 * across SQLite (WAL mode), PostgreSQL 16 (Docker), and MySQL 8.0 (Docker)
 * with audit log database sink disabled (pure query & handler performance).
 *
 * Usage:
 *   node scripts/compare-dbs.mjs
 */

import http from "node:http";
import { spawn } from "node:child_process";
import { performance } from "node:perf_hooks";

const PORT = parseInt(process.env.PORT || "8088", 10);

const DBS = [
    { name: "SQLite (WAL)", config: "config/bench-sqlite.toml" },
    { name: "PostgreSQL 16 (Docker)", config: "config/bench-postgres.toml" },
    { name: "MySQL 8.0 (Docker)", config: "config/bench-mysql.toml" },
];

const BIN_PATH =
    process.platform === "win32"
        ? ".\\target\\release\\aegis-server.exe"
        : "./target/release/aegis-server";

function httpRequest(options, body) {
    return new Promise((resolve, reject) => {
        const req = http.request(options, (res) => {
            let data = "";
            res.on("data", (chunk) => (data += chunk));
            res.on("end", () =>
                resolve({ status: res.statusCode, headers: res.headers, data }),
            );
        });
        req.on("error", reject);
        if (body) req.write(body);
        req.end();
    });
}

async function waitForServer(port = PORT, maxRetries = 40) {
    for (let i = 0; i < maxRetries; i++) {
        try {
            const res = await httpRequest({
                hostname: "127.0.0.1",
                port,
                path: "/health",
                method: "GET",
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

async function login(port = PORT) {
    const body = JSON.stringify({ username: "admin", password: "password123" });
    const res = await httpRequest(
        {
            hostname: "127.0.0.1",
            port,
            path: "/api/v1/auth/session",
            method: "POST",
            headers: {
                "Content-Type": "application/json",
                "Content-Length": Buffer.byteLength(body),
            },
        },
        body,
    );
    if (res.status === 200 || res.status === 201) {
        try {
            const json = JSON.parse(res.data);
            const token = json.id || json.token || json.session_id || "";
            const cookie = res.headers["set-cookie"]
                ? res.headers["set-cookie"][0].split(";")[0]
                : "";
            return { token, cookie };
        } catch {
            return { token: "", cookie: "" };
        }
    }
    console.error(`Login failed with status ${res.status}: ${res.data}`);
    return { token: "", cookie: "" };
}

async function runBenchEndpoint({
    path,
    method = "GET",
    headers = {},
    body = null,
    concurrency = 50,
    durationSec = 4,
}) {
    const agent = new http.Agent({
        keepAlive: true,
        maxSockets: concurrency * 2,
    });
    const latencies = [];
    const statusCounts = {};
    let totalRequests = 0;

    const reqHeaders = {
        Connection: "keep-alive",
        "User-Agent": "AegisCMS-Bench/1.0",
        ...headers,
    };
    if (body) {
        reqHeaders["Content-Length"] = Buffer.byteLength(body);
        reqHeaders["Content-Type"] = "application/json";
    }

    const reqOptions = {
        hostname: "127.0.0.1",
        port: PORT,
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
                res.on("data", () => {});
                res.on("end", () => {
                    latencies.push(performance.now() - reqStart);
                    totalRequests++;
                    statusCounts[res.statusCode] =
                        (statusCounts[res.statusCode] || 0) + 1;
                    active--;
                    fireNext();
                });
            });
            req.on("error", () => {
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
            resolve({
                totalRequests,
                elapsed,
                rps,
                p50,
                p90,
                p99,
                statusCounts,
            });
        }

        for (let i = 0; i < concurrency; i++) fireNext();
    });
}

async function main() {
    console.log(
        `=== AegisCMS Pure Performance Benchmark (Port ${PORT}, Audit Sink Disabled) ===\n`,
    );
    const results = {};

    for (const db of DBS) {
        console.log(`\n>>> Starting server with ${db.name} (${db.config})...`);
        const serverProc = spawn(BIN_PATH, ["--config", db.config, "run"], {
            stdio: "ignore",
            detached: false,
        });

        try {
            await waitForServer(PORT);
            console.log(`Server online on port ${PORT}. Running benchmarks...`);

            // 1. Public Top Page (Read-heavy, minijinja render, cache/DB read)
            process.stdout.write("  [1/4] GET / (Public index, c=50, 4s)... ");
            const rIndex = await runBenchEndpoint({
                path: "/",
                concurrency: 50,
                durationSec: 4,
            });
            console.log(
                `${rIndex.rps.toFixed(0)} req/s (p50: ${rIndex.p50.toFixed(1)}ms, p99: ${rIndex.p99.toFixed(1)}ms) [${JSON.stringify(rIndex.statusCounts)}]`,
            );

            // 2. Unauthenticated API (401 without audit sync insert)
            process.stdout.write(
                "  [2/4] GET /api/v1/contents/article (401 No-Audit, c=50, 4s)... ",
            );
            const r401 = await runBenchEndpoint({
                path: "/api/v1/contents/article",
                concurrency: 50,
                durationSec: 4,
            });
            console.log(
                `${r401.rps.toFixed(0)} req/s (p50: ${r401.p50.toFixed(1)}ms, p99: ${r401.p99.toFixed(1)}ms) [${JSON.stringify(r401.statusCounts)}]`,
            );

            // Login once to get token and cookie for authenticated API benchmark
            const { token, cookie } = await login(PORT);
            const authHeaders = {};
            if (token) authHeaders["Authorization"] = `Bearer ${token}`;
            if (cookie) authHeaders["Cookie"] = cookie;

            // 3. Authenticated Contents List API (DB query + relations, 200 OK)
            process.stdout.write(
                "  [3/4] GET /api/v1/contents/article (Auth 200 OK DB Query, c=50, 4s)... ",
            );
            const rList = await runBenchEndpoint({
                path: "/api/v1/contents/article?limit=50",
                headers: authHeaders,
                concurrency: 50,
                durationSec: 4,
            });
            console.log(
                `${rList.rps.toFixed(0)} req/s (p50: ${rList.p50.toFixed(1)}ms, p99: ${rList.p99.toFixed(1)}ms) [${JSON.stringify(rList.statusCounts)}]`,
            );

            // 4. Login POST /api/v1/auth/session (Argon2id + Session create, no audit sync insert, c=20, 3s)
            process.stdout.write(
                "  [4/4] POST /api/v1/auth/session (Login Argon2id, c=20, 3s)... ",
            );
            const loginBody = JSON.stringify({
                username: "admin",
                password: "password123",
            });
            const rLogin = await runBenchEndpoint({
                path: "/api/v1/auth/session",
                method: "POST",
                body: loginBody,
                concurrency: 20,
                durationSec: 3,
            });
            console.log(
                `${rLogin.rps.toFixed(0)} req/s (p50: ${rLogin.p50.toFixed(1)}ms, p99: ${rLogin.p99.toFixed(1)}ms) [${JSON.stringify(rLogin.statusCounts)}]`,
            );

            results[db.name] = {
                index: rIndex,
                unauth401: r401,
                list200: rList,
                login: rLogin,
            };
        } finally {
            console.log(`Stopping server for ${db.name}...`);
            serverProc.kill("SIGTERM");
            await new Promise((r) => setTimeout(r, 1000));
            try {
                process.kill(serverProc.pid, "SIGKILL");
            } catch {}
            await new Promise((r) => setTimeout(r, 1000));
        }
    }

    console.log(
        "\n========================================================================================",
    );
    console.log("RESULTS SUMMARY TABLE (Audit Sink Disabled)");
    console.log(
        "========================================================================================\n",
    );
    console.log(
        "| DB Engine | GET / (Public) | GET /api/... (401 No-Audit) | GET /api/... (Auth 200 DB) | POST /session (Login) |",
    );
    console.log("| :--- | :---: | :---: | :---: | :---: |");
    for (const [name, res] of Object.entries(results)) {
        const idx = `${res.index.rps.toFixed(0)} req/s (${res.index.p50.toFixed(1)}ms)`;
        const u401 = `${res.unauth401.rps.toFixed(0)} req/s (${res.unauth401.p50.toFixed(1)}ms)`;
        const list = `${res.list200.rps.toFixed(0)} req/s (${res.list200.p50.toFixed(1)}ms)`;
        const log = `${res.login.rps.toFixed(0)} req/s (${res.login.p50.toFixed(1)}ms)`;
        console.log(`| ${name} | ${idx} | ${u401} | ${list} | ${log} |`);
    }
}

main().catch((err) => {
    console.error("Benchmark failed:", err);
    process.exit(1);
});
