[← Back to the documentation index](README.md)

**English** | [日本語](mcp.ja.md)

# Using AegisCMS from AI Agents (MCP)

AegisCMS has a built-in [MCP](https://modelcontextprotocol.io/) (Model Context Protocol) server. An AI assistant such as Claude Code or Cursor can then read your site and write drafts, with no script in between.

- **Address**: `POST /mcp` on your site (for example `https://example.com/mcp`). It uses the Streamable HTTP transport and needs no extra setting on the server.
- **Sign-in**: an [API token](content-api.md#13-api-tokens-for-scripts-ci-and-agents) sent as `Authorization: Bearer aegis_pat_...`. Nothing else is accepted: a request without it gets `401`.
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

> [!NOTE]
> Clients that can only sign in with OAuth (for example, custom connectors in Claude.ai on the web) cannot connect yet. OAuth is planned as a later step.

## 2. Tools

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

## 3. Limits of this version

- Only these tools. Publishing, scheduling, deleting, uploading media and attaching tags are not available through MCP yet; use Admin Studio or the [REST API](content-api.md).
- There is no switch to turn the endpoint off. To stop an assistant, revoke its token.
- Only `POST /mcp` is served (no streaming from the server); `GET /mcp` answers `405`.

## Related Guides

- [Content posting API](content-api.md)
- [Authentication, API tokens](authentication.md#13-api-tokens)
- [Writing and publishing content](admin-editing.md)
