# aegis-newsletter

**English** | [日本語](README.ja.md)

AegisCMS's crate for newsletters (email magazines), double opt-in subscription, and bulk delivery (`[newsletter]`).

## Overview

`aegis-newsletter` is the crate that provides newsletter sign-up for website readers, double opt-in through a confirmation email, management of unsubscribe links, and bulk email delivery of published content.

It supports safe subscribe and unsubscribe flows that comply with anti-spam regulations (CAN-SPAM, GDPR, etc.) and tracking of send errors and delivery history.

## Responsibilities and Features

- **Double opt-in registration flow**:
  - Accepts email address registrations from readers (`pending`).
  - Sends a confirmation email containing a cryptographically secure confirmation token.
  - Confirms the subscription by verifying the token (`confirmed`).
- **Reliable unsubscribe**:
  - Generates and attaches a safe one-click unsubscribe link to each delivered email.
  - Immediate unsubscription and status management.
- **Bulk content delivery (`NewsletterService::broadcast`)**:
  - Queues published articles and news for bulk delivery to all active subscribers.
  - Safe batch sending that takes rate limits into account.
- **Delivery history and statistics**:
  - Records past delivery history, the number of successful sends, and the number of errors.

## Module Layout

| Module | Description |
| :--- | :--- |
| `lib` | `NewsletterService` implementation; subscribing, confirming, unsubscribing, and bulk delivery |
| `messages` | Localized message catalog for sign-up confirmation, unsubscribe completion, etc. (en, ja) |
| `config` | Newsletter settings (`NewsletterConfig`: sender, confirmation expiry, etc.) |
