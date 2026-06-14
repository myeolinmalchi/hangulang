# Release Notes

This repository is prepared for repeatable releases, but crates.io publishing is
blocked until `rhwp` is available as a registry dependency.

## Current Blocker

`hangulang` currently depends on `rhwp` through a pinned git revision:

```toml
rhwp = { git = "https://github.com/edwardkim/rhwp", rev = "...", default-features = false }
```

crates.io does not allow normal dependencies on code outside crates.io. Before
publishing `hangulang`, switch this dependency to a versioned registry
dependency, for example:

```toml
rhwp = { version = "0.x", default-features = false }
```

## Pre-Release Checks

Run these before cutting a release:

```bash
cargo clippy --all-targets --no-default-features -- -D warnings
cargo test --no-default-features
cargo test --features serde
cargo package --list
```

After `rhwp` is available on crates.io, add:

```bash
cargo publish --dry-run
```

## Release Steps

1. Update `Cargo.toml` version.
2. Update `CHANGELOG.md`.
3. Run the pre-release checks.
4. Create a git tag, for example `v0.1.0`.
5. Publish with `cargo publish` after the `rhwp` dependency blocker is resolved.

## Notes

- `cargo fmt --all -- --check` is intentionally not a release gate yet because
  the existing repository has not been normalized with rustfmt as a separate
  mechanical change.
- `validator-integration` requires the Python DocLang validator environment and
  should run as an optional release verification step.
