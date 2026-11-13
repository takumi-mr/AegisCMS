# aegis-cache

[English](README.md) | **日本語**

AegisCMS のキャッシュ・セッションバックエンド実装（インメモリ / Redis）、サーバー間イベントバス、および CDN パージ機構。

## 概要

`aegis-cache` は、高頻度アクセスデータのキャッシュ、セッションストレージ、クラスター構成時のサーバー間メッセージング（EventBus）、および外部 CDN（Cloudflare, Fastly 等）に対する Cache-Tag / Surrogate-Key パージ機能を提供するクレートです。

`aegis_core::CacheBackend` を実装しており、単一ノード向けの高速インメモリキャッシュから、マルチノード冗長構成向けの Redis バックエンドまでサポートします。

## 主な責務・提供機能

- **CacheBackend 実装**:
  - `MemoryCacheBackend`: TTL 付きの軽量高速インメモリキャッシュ。
  - `RedisCacheBackend`: `redis-rs` を用いた分散共有キャッシュ・セッションバックエンド。
- **イベントバス (`EventBus`)**:
  - `MemoryEventBus`: 単一ノード内のブロードキャストチャンネル。
  - `RedisEventBus`: Redis Pub/Sub を活用したマルチインスタンス間のリアルタイムイベント配信（キャッシュ無効化通知や設定同期）。
- **CDN パージ機構 (`cdn::CdnPurger`)**:
  - コンテンツ更新時に、紐づくタグ（例: `content:123`, `category:news`）に基づいて CDN のエッジキャッシュを選択的に無効化。
  - **Cloudflare**: API トークンと Zone ID による Tags パージ。
  - **Fastly**: API トークンと Service ID による Surrogate-Key パージ。
  - **Webhook**: 独自のリバースプロキシや CDN への HTTP Webhook パージ。
  - **Noop / Recording / Log**: 開発・テスト用のモックパージャー。

## モジュール構成

| モジュール | 説明 |
| :--- | :--- |
| `memory` | インメモリキャッシュ実装 (`MemoryCacheBackend`) |
| `redis` | Redis 分散キャッシュ実装 (`RedisCacheBackend`) |
| `bus` | サーバー間イベントブロードキャスト (`EventBus`, `MemoryEventBus`, `RedisEventBus`) |
| `cdn` | CDN Cache-Tag パージ機構 (`CdnPurger`, `CloudflareCdnPurger`, `FastlyCdnPurger` 等) |
| `config` | キャッシュ設定 (`CacheConfig`) |
