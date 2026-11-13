# aegis-forum

**English** | [日本語](README.ja.md)

AegisCMS's crate for the forum (bulletin board), discussions, and community features (`[forum]`).

## Overview

`aegis-forum` is a community subsystem that provides per-category thread lists, new thread creation, reply posting, pinning (sticky), thread locking (closing), and moderation.

With large, high-traffic forums in mind, its architecture lets it connect to, and be separated into, its own dedicated database (SQLite / PostgreSQL / MySQL), independent of the CMS's content database.

## Responsibilities and Features

- **Categories and permission management**:
    - Parent-child category definitions.
    - Viewing and posting policies such as member-only posting and read-only categories.
- **Thread and reply management (`ForumService`)**:
    - Creating, editing, and deleting threads.
    - Reply posting and pagination, with automatic aggregation of the last post time and reply count.
    - Pinning threads to the top (sticky) and stopping new posts (lock).
- **Moderation**:
    - Hiding inappropriate posts, and moving and merging threads by administrators and moderators.
    - Limits on consecutive posts (spam prevention).
- **Separation into a dedicated database**:
    - By specifying a dedicated database connection string in `ForumConfig`, you can run it in an independent DB without putting load on the CMS's main DB.

## Module Layout

| Module   | Description                                                                            |
| :------- | :------------------------------------------------------------------------------------- |
| `lib`    | `ForumService` implementation; category, thread, and reply operations                  |
| `config` | Forum settings (`ForumConfig`: items per page, dedicated DB connection settings, etc.) |
