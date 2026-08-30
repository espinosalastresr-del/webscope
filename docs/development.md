# Development

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo run -- model evaluate
```

Fixtures live in `tests/fixtures/` and must not contain real personal data.

CI (`.github/workflows/ci.yml`) runs fmt, check, clippy, tests and fixture evaluation on every push.
