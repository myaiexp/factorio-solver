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
cargo audit
```

### Common development commands

```sh
cargo check --workspace
cargo test --workspace
cargo audit
```
