# aegis-comments

**English** | [日本語](README.ja.md)

AegisCMS's crate for comment posting, threaded replies, moderation, and spam protection (`[comments]`).

## Overview

`aegis-comments` provides reader comment posting on published articles and pages, multi-level replies (nested threads), an administrator approval workflow, and advanced spam protection.

To keep comments safe, it integrates a keyword blacklist, detection of excessive link counts, an external AI/spam check hook through Wasm plugins (`check_comment`), and integration with the notification crate.

## Responsibilities and Features

- **Comment posting and lifecycle (`CommentService`)**:
    - Comment posting by guests or logged-in users.
    - Building a tree structure (nested replies) by specifying a parent comment ID.
    - Status transitions (`pending` awaiting approval, `approved` published, `spam`, `trash`).
- **Automatic moderation and spam detection (`moderation`)**:
    - **Keyword blacklist**: automatically blocks, or classifies as spam, comments that contain banned words and expressions.
    - **Link count limit**: automatically holds a comment when the number of URLs in the body exceeds the limit.
    - **Approval policy**: flexible settings such as manual approval for first-time posters only, manual approval for everything, or immediate publication.
- **Wasm plugin integration**:
    - Integration with external spam detection engines (Akismet, LLMs, etc.) through the `check_comment` hook.
- **Notification integration**:
    - Email notifications to administrators and posters when there are new comments or replies (via `aegis-notify`).

## Module Layout

| Module       | Description                                                                               |
| :----------- | :---------------------------------------------------------------------------------------- |
| `lib`        | `CommentService` implementation; posting, retrieving, and updating the status of comments |
| `moderation` | Banned word checks, link counting, spam score calculation logic                           |
| `config`     | Comment settings (`CommentsConfig`: approval policy, nesting depth limit, etc.)           |
