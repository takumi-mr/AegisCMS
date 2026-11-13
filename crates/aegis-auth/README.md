# aegis-auth

**English** | [日本語](README.ja.md)

AegisCMS's crate for user authentication, session management, SSO (OpenID Connect / SAML 2.0), and security features.

## Overview

`aegis-auth` provides authentication and authorization for Admin Studio administrators and for the public site's member users.

It covers secure password hashing (Argon2id), TOTP two-factor authentication (2FA), session management, rate limiting against brute-force attacks, invitation-based account registration, password reset, and enterprise external IdP integration (OpenID Connect, SAML 2.0).

## Responsibilities and Features

- **User and account management (`users`, `registration`, `invitation`)**:
    - CRUD for administrator and member users.
    - Role-based authorization (Admin, Editor, Author, Contributor, Subscriber, etc.).
    - Sign-up with email verification and issuing of invitation tokens by administrators.
- **Password security (`password`, `password_reset`)**:
    - Safe salting and hashing with the Argon2id algorithm.
    - Constant-time comparison that resists timing attacks.
    - A password reset flow with expiring one-time tokens.
- **Sessions and two-factor authentication (`session`, `totp`)**:
    - Issuing and revoking session IDs generated from cryptographically secure random numbers.
    - RFC 6238-compliant TOTP two-factor authentication (works with Google Authenticator, 1Password, etc.).
- **Brute-force protection (`rate_limit`)**:
    - Attempt limits per IP address and per user identifier, and temporary account lockout.
- **Enterprise SSO (`oidc`, `saml`)**:
    - **OpenID Connect (OIDC)**: OAuth 2.0 / OIDC integration with Google, GitHub, Okta, Azure AD, and more.
    - **SAML 2.0**: SP-initiated / IdP-initiated single sign-on with enterprise IdPs (Okta, Keycloak, Azure AD).

## Module Layout

| Module           | Description                                       |
| :--------------- | :------------------------------------------------ |
| `users`          | User data model and service classes               |
| `password`       | Password verification and hashing with Argon2id   |
| `session`        | Session lifecycle management                      |
| `totp`           | RFC 6238 TOTP two-factor authentication           |
| `rate_limit`     | Login attempt limits and rate limiter             |
| `registration`   | Public user registration flow                     |
| `invitation`     | Invitation token generation and verification      |
| `password_reset` | Password reset token management                   |
| `oidc`           | OpenID Connect auth provider (`feature = "oidc"`) |
| `saml`           | SAML 2.0 auth provider (`feature = "saml"`)       |
| `config`         | General authentication settings (`AuthConfig`)    |
