# aegis-contact

**English** | [日本語](README.ja.md)

AegisCMS's crate for contact forms, spam protection, and inbox management (`[contact]`).

## Overview

`aegis-contact` is a form engine that accepts inquiries and document requests from website visitors.

It provides flexible form definitions through the configuration file (`aegis.toml`), strict server-side input validation, bot protection (honeypot, CAPTCHA integration), storage in the inbox database, automatic purging by retention period (TTL) for privacy protection, and auto-reply email notifications to both administrators and senders.

## Responsibilities and Features

- **Configuration-driven forms (`form`, `config`)**:
  - Field definitions for name, email address, subject, type selection (select box / radio buttons), message body, and so on.
  - Required-field validation, length limits, and email address syntax checks.
  - Automatic generation of form HTML and error messages for templates.
- **Bot and spam defense**:
  - **Honeypot field**: immediately discards simple bots through a hidden input field that humans cannot see.
  - **CAPTCHA integration**: bot verification in conjunction with `aegis-captcha`.
  - **Rate limiting**: limits consecutive submissions from the same IP address.
- **Inbox and privacy management (`ContactService`)**:
  - Stores received inquiries in the internal database (`_aegis_contact_messages`).
  - Listing and viewing received messages, and read/unread management, from Admin Studio.
  - Automatic cleanup after the personal data retention period (`retention_days`).
- **Notification integration (`messages`)**:
  - Sends a thank-you email (auto-reply) on completion and an incoming-message notification email to administrators (via `aegis-notify`).

## Module Layout

| Module | Description |
| :--- | :--- |
| `lib` | `ContactService` implementation; storing, retrieving, and automatically deleting messages |
| `form` | Input validation and HTML form generation |
| `messages` | Localized error messages and notification text |
| `config` | Contact form settings (`ContactConfig`) |
