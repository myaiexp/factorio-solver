# factorio-solver

## Contributing

### Security scanning

This project uses [cargo-audit](https://github.com/rustsec/rustsec/tree/main/cargo-audit) to scan dependencies for known CVEs.

**Install cargo-audit:**

```sh
cargo install cargo-audit
```

**Run locally:**

```sh
make audit
# or directly:
cargo audit
```

**CI policy:**

- The `security` workflow runs on every push and pull request.
- It also runs on a **daily schedule** (06:00 UTC) via cron, so newly disclosed CVEs are caught even when no code changes have been made.
- A failing audit blocks merges. If an advisory is not applicable to this project, add it to `.cargo/audit.toml` under `[advisories] ignore` with a mandatory comment explaining the rationale.

### Common development commands

```sh
make check   # cargo check --workspace
make test    # cargo test --workspace
make audit   # cargo audit
```
