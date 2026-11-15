[← Back to the documentation index](README.md)

**English** | [日本語](mcp.ja.md)

# Using AegisCMS from AI Agents (MCP)

AegisCMS has a built-in [MCP](https://modelcontextprotocol.io/) (Model Context Protocol) server. An AI assistant such as Claude Code or Cursor can then read your site and write drafts, with no script in between.

- **Address**: `POST /mcp` on your site (for example `https://example.com/mcp`). It uses the Streamable HTTP transport and needs no extra setting on the server.
- **Sign-in**: either **OAuth** (the client opens a sign-in page and you click Allow; [section 2](#2-connect-with-oauth)) or an [API token](content-api.md#13-api-tokens-for-scripts-ci-and-agents) sent as `Authorization: Bearer aegis_pat_...`. A request with neither gets `401`.
- **Permissions**: the same as the REST API. The tools run as the token's role, so they go through the same checks (roles, review flow, validation) and are recorded in the [audit log](audit.md) like any other change.

> [!IMPORTANT]
> This first version can **read, and save drafts**. It cannot publish, unpublish, change the status or delete anything. Visitors never see a change made through MCP until a person publishes it in Admin Studio.

## 1. Connect

1. Issue an API token ([Admin Studio › My profile › API tokens](admin-studio.md#4-my-profile)). Choose the lowest role that is enough: `viewer` to only read, `contributor` or `editor` to write drafts.
2. Add the server to your client. For Claude Code:

    ```bash
    claude mcp add --transport http aegiscms https://example.com/mcp \
      --header "Authorization: Bearer aegis_pat_..."
    ```

    Other clients that accept a URL and a header take the same two values. If your site is served under a sub-path ([`[server].base_path`](configuration.md)), put it in front of `/mcp`.

3. Ask the assistant, for example: "List my draft articles", or "Write a draft article about our new release".

## 2. Connect with OAuth

Clients that sign in with OAuth, such as the custom connectors of Claude.ai, need only the address of the server (`https://example.com/mcp`). No token has to be made by hand:

1. The client finds the sign-in page by itself and opens it in your browser.
2. The page names the application and what it asks for. Enter your **username and password** (and the authentication code, if you use two-factor authentication) and click **Allow**. **Deny** sends you back without giving access.
3. The client receives an API token. It is valid for **30 days**; after that the client asks you to allow it again.

What the application can do is limited twice. The scope it asks for decides the role of the token: reading only (`content:read`) gives `viewer`, and drafts (`content:draft`) give `contributor`. And the token never goes above your own role. Neither scope can publish or delete.

The token appears in **My profile › API tokens** as `OAuth: <application name>`, and you can revoke it there at any time. Allowing the same application again replaces its earlier token.

> [!NOTE]
>
> - Sign-in on this page uses a username and password. An account that can only sign in through single sign-on cannot allow an application yet; use an [API token](#1-connect) instead.
> - The site should be served over `https://` and have its **Canonical URL** set ([Site settings](admin-settings.md)), because the addresses of the sign-in page given to the client are built from it.
> - The endpoints are `/.well-known/oauth-protected-resource`, `/.well-known/oauth-authorization-server`, `/oauth/register` (dynamic client registration), `/oauth/authorize` and `/oauth/token`. PKCE (`S256`) is required.

## 3. Tools

| Tool                 | What it does                                                                                                                                                           |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `list_content_types` | The content types of the site (articles, pages, ...).                                                                                                                  |
| `get_content_schema` | The fields of one content type.                                                                                                                                        |
| `list_content`       | Items of a type, newest first, as summaries without the body. Filter with `q`, `status`, `tag` and `category`; page with `limit` (default 20, up to 100) and `offset`. |
| `get_content`        | One item with its body blocks and custom fields, by slug or id.                                                                                                        |
| `list_tags`          | The tags of the site.                                                                                                                                                  |
| `create_draft`       | A new item, always saved as a **draft**. Needs `content_type`, `title` and `slug`; `blocks` and `fields` are optional.                                                 |
| `update_content`     | Changes an existing item. Send only what changes: `title`, `slug`, `blocks` (replaces the whole body) and `fields` (merged into the current custom fields).            |

What reading shows depends on the token's role, as with the REST API: a `viewer` token sees what a viewer sees in Admin Studio.

### How writing behaves

- **`create_draft` always saves a draft.** A `status` or `publish` sent along is ignored. The slug must be unique within the type.
- **`update_content` keeps the status.** For an item that is published, the change is saved as a **working copy**: visitors keep seeing the published version until a person clicks **Update the published version** in Admin Studio ([Writing and publishing content](admin-editing.md#4-status-and-publishing)).
- A token whose role may not write, or may not edit someone else's item, gets the same refusal as the REST API (`403`).
- The body is a list of blocks, in the same format as the REST API ([Body block specification](content-api.md#3-body-block-specification-blocks)). Ask the assistant to read `get_content` of a similar item first if it is unsure of the shape.

When a tool fails, the assistant receives the HTTP status and the error message of the REST API (for example `409` for a slug that is already used, `404` for an item that does not exist).

## 4. Limits of this version

- Only these tools. Publishing, scheduling, deleting, uploading media and attaching tags are not available through MCP yet; use Admin Studio or the [REST API](content-api.md).
- OAuth gives a token valid for 30 days and has no refresh tokens yet, so the client asks you to allow it again when it expires. The sign-in page is in English only.
- There is no switch to turn the endpoint off. To stop an assistant, revoke its token.
- Only `POST /mcp` is served (no streaming from the server); `GET /mcp` answers `405`.

## Related Guides

- [Content posting API](content-api.md)
- [Authentication, API tokens](authentication.md#13-api-tokens)
- [Writing and publishing content](admin-editing.md)
