# aegis-schema

**English** | [日本語](README.ja.md)

Declarative YAML schema definitions, multi-dialect DDL auto-migration, and a TypeScript / OpenAPI 3.1 code generation engine.

## Overview

In AegisCMS, content types (posts, products, events, portfolios, etc.) are defined as declarative schemas in YAML files. This lets you version-control schemas in Git (Code-as-Configuration).

`aegis-schema` loads these schema definitions, automatically generates and runs DDL migrations for the internal RDBMS (SQLite, PostgreSQL, MySQL), and also generates TypeScript type definitions for the frontend and OpenAPI 3.1 specifications.

## Responsibilities and Features

- **Declarative schema definitions (`definition`)**:
  - `SchemaDefinition`, `FieldDefinition`, `FieldType` (string, text, integer, boolean, datetime, json, relation, etc.).
  - Transparent integration of the standard common columns (`id`, `title`, `slug`, `status`, `created_at`, `updated_at`, `published_at`, etc.).
  - Relation definitions (`RelationTarget`: has_one, has_many, belongs_to).
- **Schema loader (`loader`)**:
  - Scans, parses, and validates the consistency of YAML/JSON schema files under the `schemas/` directory.
- **Multi-dialect migration (`migration`)**:
  - Diff detection and automatic DDL generation for SQLite, PostgreSQL, and MySQL (`CREATE TABLE`, `ALTER TABLE ADD COLUMN`, index creation).
- **Code generation (`codegen`)**:
  - **TypeScript Generator**: Generates TypeScript interface definitions for Admin Studio and external frontends.
  - **OpenAPI 3.1 Generator**: Automatically outputs RESTful API endpoints and schema specifications (JSON/YAML) based on the schema definitions.

## Module Layout

| Module | Description |
| :--- | :--- |
| `definition` | Data structures for schemas, fields, and relations |
| `loader` | Loading schemas from the file system and syntax validation (`SchemaLoader`) |
| `migration` | Migration DDL generation for each SQL dialect (`SchemaMigration`, `SqlDialect`) |
| `codegen` | TypeScript type definitions and OpenAPI 3.1 specification generation (`TypeScriptGenerator`, `OpenApiGenerator`) |

## Usage Example

```rust
use aegis_schema::{SchemaLoader, SqlDialect};

// 1. Load schema files
let loader = SchemaLoader::new();
let schemas = loader.load_from_dir("schemas/")?;

// 2. Generate DDL migration scripts for the database
for schema in &schemas {
    let ddl = schema.to_ddl(SqlDialect::Sqlite);
    println!("Migration SQL:\n{}", ddl);
}

// 3. Output TypeScript definitions
let ts_code = aegis_schema::codegen::TypeScriptGenerator::generate(&schemas)?;
```
