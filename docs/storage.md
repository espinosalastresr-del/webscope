# Storage

SQLite with WAL, foreign keys and versioned migrations (`schema_migrations`).

## Core tables

`urls`, `pages`, `detections`, `patterns`, `pattern_results`, `training_samples`, `models`, `crawl_runs`, `errors`.

## Resume

URL status machine: `discovered → queued → processing → completed|failed|skipped|retry`.

A new `webscope scan` continues from persisted state instead of starting over.
