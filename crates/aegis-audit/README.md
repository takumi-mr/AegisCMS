# aegis-audit

**English** | [日本語](README.ja.md)

AegisCMS's audit log delivery service and implementations of the various output sinks.

## Overview

`aegis-audit` is the crate that captures audit events such as admin operations, authentication events, content changes, plugin executions, and settings updates, and persists and delivers them safely.

To meet enterprise security requirements, it provides tamper resistance, fan-out delivery to multiple sinks, automatic cleanup based on a retention period (TTL), and Prometheus metrics integration.

## Responsibilities and Features

- **AuditService**:
  - Asynchronously receives audit events (`AuditEvent`) from across the application and delivers them to the sinks in bulk.
  - Attaches the origin context (`AuditOrigin`: IP address, User-Agent, request ID).
  - Automatically purges old logs past the retention period (`retention_days`).
- **Output sinks (`AuditSink`)**:
  - `DatabaseAuditSink`: structured persistence in the database (the `_aegis_audit_logs` table). Searchable and viewable from Admin Studio.
  - `LogAuditSink`: JSON output to the host's `tracing` log (or syslog / stdout). Ideal for SIEM integration.
  - `PrometheusAuditSink`: aggregates and exposes the frequency of audit events and anomalous events (authentication failures, access denials, etc.) as Prometheus counter metrics.

## Module Layout

| Module | Description |
| :--- | :--- |
| `database` | Database output sink (`DatabaseAuditSink`) |
| `log` | Structured logging output sink (`LogAuditSink`) |
| `metrics` | Prometheus metrics output sink (`PrometheusAuditSink`) |
| `origin` | Request context extraction (`AuditOrigin`) |
| `config` | Audit settings (`AuditConfig`, `AuditSinkKind`) |
