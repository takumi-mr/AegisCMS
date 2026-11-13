# aegis-db

**English** | [日本語](README.ja.md)

AegisCMS's database abstraction layer (`DatabaseAdapter`) and its SQLite / PostgreSQL / MySQL implementations.

## Overview

`aegis-db` is the data access layer crate responsible for persisting content data, system settings, orders and inventory, comments, the forum, tags, and more.

It provides the dialect-independent abstract trait `DatabaseAdapter` and supports seamless switching from the Zero-Config embedded SQLite to PostgreSQL and MySQL for large-scale operation.

## Responsibilities and Features

- **The DatabaseAdapter abstract trait**:
    - Transaction control, dynamic query execution, and schema migration execution.
    - Complete elimination of SQL injection through prepared statements.
- **Multi-RDBMS adapters**:
    - `SqliteAdapter`: embedded SQLite (rusqlite). Runs Zero-Config on a file or in memory. WAL mode and foreign key constraints are enabled automatically.
    - `PostgresAdapter`: supports PostgreSQL / TimescaleDB.
    - `MySqlAdapter`: supports MySQL / MariaDB.
- **Storage implementations for each feature**:
    - `orders` / `variants`: persistence of e-commerce orders, inventory management, and product variants.
    - `comments`: comment posting, pending approval, and thread hierarchy queries.
    - `forum`: forum categories, threads, replies, pinning, and moderation.
    - `tags`: attaching tags across content and reverse lookups.
    - `contact` / `newsletter`: management of the contact inbox and newsletter subscribers.
    - `analytics`: daily storage of cookie-less aggregated data.
    - `notifications`: the notification outbox queue.

## Module Layout

| Module                   | Description                                               |
| :----------------------- | :-------------------------------------------------------- |
| `sqlite`                 | SQLite adapter implementation (`SqliteAdapter`)           |
| `postgres`               | PostgreSQL adapter implementation (`PostgresAdapter`)     |
| `mysql`                  | MySQL adapter implementation (`MySqlAdapter`)             |
| `orders` / `variants`    | Data access for orders, inventory, and variants           |
| `comments`               | Storing, searching, and moderating comment data           |
| `forum`                  | Storing and searching forum data                          |
| `tags`                   | Operations on tags and the content association tables     |
| `contact` / `newsletter` | Operations on contact form and newsletter subscriber data |
| `analytics`              | Persistence of daily analytics data                       |
| `notifications`          | Persistence of the notification outbox queue              |
| `config`                 | Database connection settings (`DatabaseConfig`)           |
