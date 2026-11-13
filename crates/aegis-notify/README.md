# aegis-notify

**English** | [日本語](README.ja.md)

AegisCMS's notification delivery engine, localized templates, and outbox queue worker crate (`[notifications]`).

## Overview

`aegis-notify` is a notification subsystem that receives the various events that occur in the system (new orders, member registration, password reset, comment notifications, contact form receipts, etc.) and delivers them to the appropriate channel (SMTP email or log).

It implements the `aegis_core::Notifier` trait and has multipart HTML/text message generation in multiple languages (English and Japanese) through a template engine, a persistent outbox queue backed by the database, and a background retry worker.

## Responsibilities and Features

- **NotificationService**:
    - The host implementation of `aegis_core::Notifier`.
    - Processes send requests immediately or asynchronously through the DB outbox queue (`_aegis_notifications`).
    - A background worker periodically polls the unsent queue and retries sending.
- **Delivery channels (`channel`)**:
    - **SMTP Email (`email`)**: SMTP email sending with STARTTLS / TLS support through the `lettre` crate.
    - **Log (`log`)**: log output for development and testing (writes to the `tracing` log instead of actually sending email).
- **Localized multipart templates (`templates`, `html`)**:
    - Built-in standard system notification templates in English (en) and Japanese (ja).
    - Simultaneous generation of plain text and a polished responsive HTML email.
    - Covers standard events such as order confirmation, shipping notices, password reset, invitations, and new comments.
- **Template preview (`samples`)**:
    - Provides mock samples so you can check how notification emails look from Admin Studio.

## Module Layout

| Module      | Description                                                                |
| :---------- | :------------------------------------------------------------------------- |
| `lib`       | `NotificationService` implementation; send queueing and the worker task    |
| `email`     | SMTP email transport using `lettre`                                        |
| `channel`   | Abstract interface for delivery channels (`NotificationChannel`)           |
| `templates` | Definition and rendering of templates per event and locale                 |
| `html`      | Layout and style injection for HTML emails                                 |
| `config`    | SMTP server connection and sender address settings (`NotificationsConfig`) |
