# Architecture

WebScope is organized by responsibility so that crawling, discovery, detection and learning remain independent.

## Modules

| Module | Responsibility |
|--------|----------------|
| `cli` | Clap command definitions |
| `config` | Defaults, TOML, env, CLI priority |
| `storage` | SQLite schema, migrations, records |
| `cache` | URL normalisation and deduplication |
| `discovery` | Autonomous URL discovery (seeds + adaptive patterns) |
| `crawler` | Tokio workers, limits, retries, graceful shutdown |
| `detectors` | Payment, account, personal data, CAPTCHA, Cloudflare |
| `features` | Structural feature extraction (no user values) |
| `patterns` | Pattern scoring, promotion and discard |
| `learning` | Metrics, evaluation on fixtures, active-learning samples |
| `models` | Model version metadata |
| `output` | Human and JSON summaries |
| `telemetry` | tracing setup |

## Data flow

1. **Discovery** inserts candidate URLs (`status=queued`) without downloading.
2. **Crawler** workers pull queued URLs, fetch with size/timeout limits, write `pages`.
3. **Detectors** run on public HTML/headers only → `detections`.
4. Low-confidence results become `training_samples` (`source=uncertain`).
5. Pattern productivity is updated from detection outcomes (precision-weighted).
6. Next discovery run ranks paths by pattern score (adaptive).

## Reliability invariants

- No form submission, no credential collection, no CAPTCHA solving.
- SQLite WAL + explicit status machine enable resume after crash/suspend.
- Evaluation metrics (precision/recall) are computed on labelled local fixtures only.
- Primary quality target: **payment precision ≥ 0.90**.
