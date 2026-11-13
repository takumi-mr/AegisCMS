# aegis-server

**English** | [日本語](README.ja.md)

AegisCMS's HTTP server, CLI entry point, REST API routing, and overall integration crate.

## Overview

`aegis-server` integrates all the domain crates (`aegis-db`, `aegis-storage`, `aegis-auth`, `aegis-runtime`, `aegis-theme`, etc.) and provides an Axum-based HTTP server and CLI tool that runs as a single binary.

It oversees SSR rendering of the public website, static asset delivery for Admin Studio (a React SPA), the content management RESTful API, security header injection, authorization middleware, and a scheduled task scheduler (scheduled publishing and periodic backups).

## Responsibilities and Features

- **CLI commands (`main.rs`)**:
    - `aegis-server run`: start the core server.
    - `aegis-server migrate`: run automatic schema migration.
    - `aegis-server dump` / `restore`: one-shot backup and restore of the database and storage.
    - `aegis-server theme dev`: start the local theme development server.
    - `aegis-server plugin dev`: start the local Wasm plugin development server.
    - Administration utilities such as creating administrator users and resetting passwords.
- **HTTP / REST API routing (`app.rs`, `handlers/`)**:
    - APIs for content CRUD, media upload, schema retrieval, plugin settings, comments, e-commerce orders, audit logs, and more.
- **Public page rendering (`render.rs`)**:
    - MiniJinja SSR rendering through `aegis-theme`, meta tag / SEO output, and application of Wasm plugin hooks (`on_render_page`).
- **Admin Studio delivery**:
    - Serves the embedded React-based SPA, with an HTML5 History API fallback.
- **Security middleware**:
    - Security headers (CSP, HSTS, X-Content-Type-Options, X-Frame-Options).
    - Session authentication with Cookie / Bearer Token and RBAC authorization.
    - Rate limiting, CORS, and automatic context extraction for audit logs (`AuditOrigin`).
- **Integrated configuration (`config::AegisConfig`)**:
    - Parses and manages the settings of all domain crates (DB, storage, cache, authentication, e-commerce, etc.) from `aegis.toml` in one place.

## Module Layout

| Module      | Description                                                                       |
| :---------- | :-------------------------------------------------------------------------------- |
| `app`       | Axum Router construction and middleware chain setup                               |
| `config`    | Definition and loading of the overall settings `AegisConfig`                      |
| `handlers`  | REST API endpoint handlers (content, users, media, e-commerce, etc.)              |
| `render`    | Template rendering of public pages, SEO meta, and plugin hook composition         |
| `auth`      | Authentication and authorization middleware, session validation                   |
| `audit`     | Extracts the audit log context (`AuditOrigin`) from request information           |
| `scheduler` | Background task scheduler for scheduled publishing, automatic trash purging, etc. |
| `headers`   | Construction of Content-Security-Policy and other security headers                |
| `state`     | Application shared state (`AppState`)                                             |
