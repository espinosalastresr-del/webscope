# WebScope

CLI for **autonomous web discovery, crawling, classification and incremental learning**.

Written in Rust. Targets constrained environments (iSH / Alpine / musl) as well as standard Linux.

## Principles

- **Passive analysis only** — never submits forms; never collects card numbers, passwords, tokens or user-entered values.
- **No CAPTCHA / Cloudflare bypass** — detection only.
- **No initial URL or query required** — Discovery Engine uses allowed public sources and learned patterns.
- **Resumable** — SQLite-backed state survives suspension and crashes.
- **Reliability first** — primary quality target is **payment precision ≥ 0.90** on labelled fixtures; speed is secondary.

## Quick start

```bash
cargo build --release
./target/release/webscope scan --limit 1000 --threads 4
./target/release/webscope status
./target/release/webscope stats
./target/release/webscope model evaluate
./target/release/webscope version
```

## Commands

| Command | Description |
|---------|-------------|
| `scan` | Discover + crawl + classify |
| `status` | Progress of current/last run |
| `stats` | Aggregated detection statistics |
| `review` | Active-learning review of uncertain samples |
| `cache stats` / `cache clear --confirm` | Cache management |
| `patterns list` / `patterns stats` | Pattern engine |
| `model train` / `model evaluate` | Offline evaluation / training hooks |
| `export --format json\|csv` | Export detections |
| `version` | Version + architecture |

### Scan options

```
--limit N          Max *new* URLs this run (default 1000)
--threads N        Concurrent workers (1–64)
--refresh          Re-fetch already completed URLs
--max-size BYTES   Response size limit
--timeout SECS     Request timeout
--retries N        Max retries
--json             Machine-readable summary
-q / -v            Logging
```

## Architecture

See [docs/architecture.md](docs/architecture.md).

```
Discovery → queue (SQLite) → crawler workers → detectors → detections
                                      ↓
                              training_samples (uncertain)
                                      ↓
                              pattern scores → next discovery
```

## Configuration

Optional `webscope.toml` (CLI overrides file; env overrides defaults):

```toml
threads = 4
timeout_secs = 30
max_response_size = 2097152
database = "webscope.db"
learning = true
discovery = true
cache = true
log_level = "info"
```

Env: `WEBSCOPE_THREADS`, `WEBSCOPE_TIMEOUT`, `WEBSCOPE_DB`, `WEBSCOPE_USER_AGENT`, `WEBSCOPE_LOG`.

## Evaluation

```bash
webscope model evaluate
```

Runs offline on `tests/fixtures`. Reports accuracy, precision, recall, F1 and confusion counts per category. **Payment precision ≥ 0.90** is the primary acceptance metric.

## iSH / Alpine

Heavy builds run in GitHub Actions (`.github/workflows/build-ios.yml`). Download the musl artifact, verify SHA256, `chmod +x`, run. Prefer low `--threads` on device. Details: [docs/ish.md](docs/ish.md).

## Responsible behaviour

- Identifiable User-Agent
- Rate limits and `Retry-After` respected
- No authenticated access, no protection evasion
- Sites that reject the crawler are marked and skipped

## License

MIT
