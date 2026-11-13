# aegis-tags

**English** | [日本語](README.ja.md)

AegisCMS's crate for tag management and associating tags with content (`[tags]`).

## Overview

`aegis-tags` provides managed tags that can be attached across all content types, such as articles, static pages, and products.

It manages tag names, unique slugs (for URL permalinks), descriptions, and the many-to-many relation with content, and efficiently retrieves and aggregates article lists per tag.

## Responsibilities and Features

- **Tag CRUD management (`TagService`)**:
    - Creating, updating, and deleting tags.
    - Preventing duplicate registration of tags with the same name, with case-insensitive normalization.
- **URL-safe slug generation**:
    - Automatically derives a slug made of ASCII alphanumerics and hyphens (for `/tags/{slug}`) from the tag name.
    - Falls back to a unique slug (such as `tag-<hash>`) even for tag names made up only of Japanese or other non-Latin characters.
- **Assigning tags to content**:
    - Bulk setting (attaching and detaching) of tags when content is updated.
    - Prevents spam and excessive tagging with a limit on the maximum number of tags per content (`max_per_content`).
- **Aggregation and reverse lookup**:
    - Counts the content associated with each tag (for tag cloud display).
    - Paginated retrieval of the content list that has a given tag.

## Module Layout

| Module   | Description                                                                   |
| :------- | :---------------------------------------------------------------------------- |
| `lib`    | `TagService` implementation, automatic slug generation logic, validation      |
| `config` | Tag settings (`TagsConfig`: maximum number of tags, default sort order, etc.) |
