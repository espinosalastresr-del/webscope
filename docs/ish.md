# Running on iSH (iOS)

## Constraints

- Limited RAM/CPU
- App suspension
- Often 32-bit x86 (i686) userspace historically
- Unstable connectivity

WebScope is designed around SQLite checkpoints, resumable scans, and configurable `--threads`.

## Install from CI artifact

1. Download the `webscope-ish` artifact from GitHub Actions (`build-ios.yml`).
2. Prefer `webscope-i686-unknown-linux-musl` when available; otherwise `x86_64-unknown-linux-musl`.
3. Verify SHA256 against `SHA256SUMS`.
4. `chmod +x webscope-*` and place on `$PATH` (e.g. `~/bin`).
5. Run `webscope version` and confirm architecture.

Do **not** rely on compiling inside iSH; builds are produced on GitHub Actions.

## Recommended scan settings on device

```bash
webscope scan --limit 200 --threads 2
```

Increase threads only if the device remains responsive.
