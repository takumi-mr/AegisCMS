# aegis-analytics

**English** | [日本語](README.ja.md)

A privacy-first, cookie-less analytics engine (`feature = "analytics"`).

## Overview

`aegis-analytics` is a lightweight analytics crate that counts page views and unique visitors with no user tracking and no cookies at all, with GDPR and personal information protection laws in mind.

It strictly respects HTTP request headers (Do Not Track / Global Privacy Control) and never persists raw IP addresses or User-Agents in the database. Only daily statistics are aggregated and stored.

## Responsibilities and Features

- **Privacy-protecting design**:
  - **Cookie-less**: stores no cookies or localStorage tracking tokens on the device.
  - **Respects DNT / GPC**: if the browser's `DNT: 1` or `Sec-GPC: 1` header is set, the analytics data is discarded (not counted).
  - **Salted daily hash**: daily visitors are de-duplicated in memory, only within the day, using a hash of "a random salt valid for that day only + IP address + User-Agent". The salt is discarded the next day, so matching against the past is impossible.
- **Lightweight beacon (`beacon_script`)**:
  - Serves an inline JavaScript beacon of only a few lines on page load.
  - Sends only the viewed page path and the referrer host, asynchronously.
- **Efficient aggregation and batched writes**:
  - Instead of writing each access to the DB one by one, it aggregates in an in-memory buffer and flushes to the database in batches periodically.
  - Provides statistics by page, by day, by referrer, and by device type (desktop / mobile / tablet / bot).
- **Bot detection (`agent`)**:
  - Identifies search engine crawlers and automated scanning bots and separates them from real user traffic.

## Module Layout

| Module | Description |
| :--- | :--- |
| `agent` | User-Agent parsing, device type detection, crawler and bot detection |
| `config` | Analytics settings (`AnalyticsConfig`) |
| `lib` | Aggregation service class, in-memory buffer, beacon script delivery |
