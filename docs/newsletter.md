[← Back to the documentation index](README.md)

**English** | [日本語](newsletter.ja.md)

# Newsletter

Lets readers register their email address and receive notices of new articles by email (`[newsletter]`). After signing up, a confirmation email arrives, and the subscription starts **only once they open its link** (double opt-in). Each email includes an unsubscribe link. In Admin Studio's **Settings › Newsletter**, administrators can send a published article to all subscribers.

The feature lives in the `aegis-newsletter` crate. Emails are sent through the `[notifications]` outbox ([Notifications](notifications.md)), so **SMTP must be configured first**.

```text
 Reader ── the form at /newsletter (or POST /api/v1/newsletter/subscribe)
        │  email address
        ▼
 Invisible bot-trap field / CAPTCHA (`newsletter` in `[captcha]` `targets`) / sign-ups per IP ── refuse, or discard while looking like success
        │
 New address, or an unsubscribed one ──▶ saved as pending → confirmation email newsletter.confirm
 Registered, or pending (within 1 minute) ──▶ nothing is sent (the response does not reveal which state it is)
        │  link in the email /newsletter/confirm?token=… (opens a page with a button; pressing it confirms)
        ▼
 Subscribed (active) ◀── the administrator picks an article and sends ── newsletter.issue to each subscriber one by one (with an unsubscribe link)
        │  link in the email /newsletter/unsubscribe?token=… (opens a page with a button; pressing it stops)
        ▼
 Stopped (unsubscribed)
```

---

## 1. Enabling It

```toml
[commerce]
public_url = "https://example.com"   # URL of the public site used in email links (required)

[notifications]
enabled = true                        # send confirmation emails and deliveries by email (for SMTP settings, see the notifications document)
channels = ["email"]

[newsletter]
enabled = true
locale = "ja"                         # language of the sign-up, confirmation, and unsubscribe pages (en / ja)
```

- With `[newsletter].enabled = true`, `/newsletter`, the API, and the Admin Studio screen become active. If `[commerce].public_url` is empty, email links cannot be built, so **the server does not start**. When `[notifications]` is disabled, it starts but can send neither confirmation emails nor deliveries (a warning is logged).
- The email text (subject and body) is the notification templates `newsletter.confirm` / `newsletter.issue`. They can be edited and test-sent in Admin Studio's **Settings › Notifications** ([Notifications 6](notifications.md#6-managing-from-admin-studio)).
- The paths of the sign-up page `/newsletter`, confirmation `/newsletter/confirm`, and unsubscribe `/newsletter/unsubscribe` cannot be changed. `/newsletter` cannot be used for `admin_path` either.

| Key                   | Type    | Default | Description                                                                                          |
| --------------------- | ------- | ------- | ---------------------------------------------------------------------------------------------------- |
| `enabled`             | bool    | `false` | `true` enables it                                                                                    |
| `max_per_ip_per_hour` | integer | `10`    | Number of sign-ups per hour from the same IP (0 is unlimited; counted in `[cache]`)                  |
| `confirm_ttl_hours`   | integer | `48`    | Expiry of the confirmation link (hours; 1–336). After it passes, sign up again with the same address |
| `locale`              | string  | `"en"`  | Language of the text on the sign-up, confirmation, and unsubscribe pages. `en` / `ja`                |

## 2. CAPTCHA (Optional)

Add CAPTCHA only if the sign-up form is being targeted by bots. How to write it, the fail-closed default, and the automatic CSP additions are the same as for the contact form ([Contact form 1.2](contact.md#12-captcha-optional)).

```toml
[captcha]
targets = ["newsletter"]
provider = "turnstile"
site_key = "0x4AAAA..."
secret = "env:TURNSTILE_SECRET"
```

The same check applies to the JSON API, and without the token field (such as `cf-turnstile-response`) it returns `400 captcha_failed`.

## 3. The Sign-Up Page

`/newsletter` is displayed with the theme's `page.html` (rendered with the description and form in the body). To change the look, change `page.html`, or specify `.aegis-newsletter-form`, `.aegis-newsletter-submit`, `.aegis-newsletter-notice`, and `.aegis-newsletter-sent` in CSS (the minimal styles are applied with zero specificity, so the theme's CSS takes precedence). Link to it from the menu or footer.

When placing your own form, submit it to `POST /newsletter/subscribe` in this shape. **Always include the invisible field `website`, and hide it so people cannot see it** (if a bot fills it, the request is discarded with the same response as success).

```html
<form method="post" action="/newsletter/subscribe">
  <div style="position:absolute;left:-10000px" aria-hidden="true">
    <input type="text" name="website" tabindex="-1" autocomplete="off">
  </div>
  <label>Email <input type="email" name="email" required></label>
  <button type="submit">Subscribe</button>
</form>
```

When accepted, it moves to `/newsletter?sent=1` ("We sent a confirmation email"). When there is a `[server].base_path`, add it to `action` too.

### 3.1 Confirmation and Unsubscribe Links

Opening a link in an email shows **a page with a button**. Pressing the button (`POST`) carries it out. This is so that security software or previews that open emails automatically cannot confirm or stop a subscription on their own.

- The confirmation link's expiry is `confirm_ttl_hours`. Expired, used, or nonexistent links all show the same "invalid or expired" page (404).
- Once unsubscribed, the same subscriber's old confirmation links cannot be used. If a person who unsubscribed signs up again, it starts over from confirmation (with a new link).
- Sign-ups that stay pending for 7 days are removed by a cleanup that runs once a day.

## 4. Handling of Sign-Ups (Privacy)

- **It does not reveal whether someone is registered**: the response to a sign-up is the same for a new address, a registered one, a pending one, and an unsubscribed one. Nothing is sent to an already registered address (this makes it harder to have someone enter another person's address and cause spam to be sent to them).
- A pending address gets another confirmation email only **at least 1 minute** after the previous one. The per-IP sign-up count (`max_per_ip_per_hour`) applies too.
- What is stored is the email address, status, the tokens for the confirmation and unsubscribe links, the IP address at sign-up, and the time. The IP address appears only in Admin Studio's list. Tokens appear neither in the API nor on screen.
- Subscribers are included in backups (`database/newsletter_subscribers.json`, `newsletter_issues.json`). It is personal information, so be careful with how backups are handled.

## 5. Admin Studio (Settings › Newsletter, `admin`)

- **Counts per status** (subscribed, awaiting confirmation, stopped).
- **Sending content**: choose a published article, check the summary (if empty, the article's `excerpt` field, or else the first paragraph), **send a test to yourself first**, and then send to all subscribers. A record (history) of what was sent is kept, and if you try to send the same article again, you can resend after confirming.
    - The email contains the title, summary, the article's URL, and a per-subscriber unsubscribe link.
    - What is sent is the **version currently published** (changes that are only saved and not published are not included).
    - Sending goes into the notification outbox and is not lost even if the server restarts. Because one email is sent per subscriber, check your SMTP server's daily sending limit.
- **Subscribers**: a list (newest sign-ups first, filter by status, search by email address), delete (removes the record entirely), and **CSV export** (the email addresses of people who receive deliveries).
- **Send history**: the articles sent and the number of recipients.

## 6. API

| API                                                            | Permission | Description                                                                                                                                                                                                                                                                                                                                                                                                                          |
| -------------------------------------------------------------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `POST /api/v1/newsletter/subscribe`                            | None       | `{"email": "...", "website": ""}`. When accepted, `202 {"status": "pending"}` (the same even if already registered). Mistakes are `400 invalid_email` and `429 rate_limited`; CAPTCHA gives `400 captcha_failed` / `503 captcha_unavailable`                                                                                                                                                                                         |
| `GET /api/v1/newsletter/subscribers?status=&q=&limit=&offset=` | admin      | Subscribers (newest sign-ups first), the total, and the counts per status `counts`. Tokens are not included                                                                                                                                                                                                                                                                                                                          |
| `POST /api/v1/newsletter/subscribers/delete`                   | admin      | `{"ids": [...]}` (1–500 items). `{"count": n}`                                                                                                                                                                                                                                                                                                                                                                                       |
| `GET /api/v1/newsletter/subscribers/export`                    | admin      | A CSV of the email addresses of people who receive deliveries (`text/csv`, as a download)                                                                                                                                                                                                                                                                                                                                            |
| `GET /api/v1/newsletter/issues?limit=&offset=`                 | admin      | The send history (newest first) and the total                                                                                                                                                                                                                                                                                                                                                                                        |
| `POST /api/v1/newsletter/issues`                               | admin      | `{"content_type": "article", "slug_or_id": "...", "summary": "", "test_to": "", "force": false}`. With `test_to`, it sends only to that address (`202`, not recorded in history). Sending to all subscribers returns `201` (one history entry). An already sent one returns `409 already_sent` (resend with `force: true`), no subscribers returns `409 no_subscribers`, and unpublished content returns `400 content_not_published` |

Errors are in English and carry a machine-readable `code`. Sending and deleting are recorded in the audit log (`newsletter.send` / `newsletter.delete`).

## 7. Behavior Rules

- **The same article is never sent twice at once** (double-clicking the button, multiple administrators). While another send is in progress, it returns `503 unavailable`.
- Delivery loads subscribers 500 at a time while handing them to notifications, so memory is not exhausted even with many subscribers. If the server stops partway, the ones already in the notification outbox are sent after restart, but subscribers not yet handed over do not receive it (the history is recorded at the end, so in that case you can resend with `force`; people who already received it get a duplicate).
- The unsubscribe URL appears only in the email body. The `List-Unsubscribe` header (one-click unsubscribe) is not added (because the email channel does not support adding headers).
