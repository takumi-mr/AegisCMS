# aegis-cache

**English** | [日本語](README.ja.md)

Cache and session backend implementations for AegisCMS (in-memory / Redis), an inter-server event bus, and a CDN purge mechanism.

## Overview

`aegis-cache` is the crate that provides caching of frequently accessed data, session storage, inter-server messaging in clustered setups (EventBus), and Cache-Tag / Surrogate-Key purging for external CDNs (Cloudflare, Fastly, etc.).

It implements `aegis_core::CacheBackend` and supports everything from a fast in-memory cache for a single node to a Redis backend for redundant multi-node setups.

## Responsibilities and Features

- **CacheBackend implementations**:
    - `MemoryCacheBackend`: a lightweight, fast in-memory cache with TTL.
    - `RedisCacheBackend`: a distributed shared cache and session backend using `redis-rs`.
- **Event bus (`EventBus`)**:
    - `MemoryEventBus`: a broadcast channel within a single node.
    - `RedisEventBus`: real-time event delivery between multiple instances using Redis Pub/Sub (cache invalidation notifications and settings sync).
- **CDN purge mechanism (`cdn::CdnPurger`)**:
    - When content is updated, selectively invalidates the CDN's edge cache based on the associated tags (e.g. `content:123`, `category:news`).
    - **Cloudflare**: Tags purge with an API token and Zone ID.
    - **Fastly**: Surrogate-Key purge with an API token and Service ID.
    - **Webhook**: HTTP webhook purge to your own reverse proxy or CDN.
    - **Noop / Recording / Log**: mock purgers for development and testing.

## Module Layout

| Module   | Description                                                                                 |
| :------- | :------------------------------------------------------------------------------------------ |
| `memory` | In-memory cache implementation (`MemoryCacheBackend`)                                       |
| `redis`  | Redis distributed cache implementation (`RedisCacheBackend`)                                |
| `bus`    | Inter-server event broadcast (`EventBus`, `MemoryEventBus`, `RedisEventBus`)                |
| `cdn`    | CDN Cache-Tag purge mechanism (`CdnPurger`, `CloudflareCdnPurger`, `FastlyCdnPurger`, etc.) |
| `config` | Cache settings (`CacheConfig`)                                                              |
