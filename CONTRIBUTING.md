# Contributing

This document lists practical rules for development and testing in this
repository. The project aims for correctness first and measurable performance
second. Keep changes small, well-tested, and incremental.

## Development rules

- Use the `main` branch for the canonical source; open pull requests with a
  clear description of intent and scope.
- Keep changes minimal and focused. One feature or fix per PR.
- Do not change the core key-generation algorithm or `validate_for_meshcore`
  semantics without explicit approval from a project maintainer.

## Formatting & linting

Run the toolchain checks before opening a PR:

```bash
cargo fmt --all
cargo clippy --all -- -D warnings
cargo test
```

## Testing expectations

- Unit tests are present in `src/*` modules. Every behavioral change must be
  accompanied by unit tests that assert the intended behavior.
- Run `cargo test` locally and ensure tests pass on supported platforms.
- For performance-sensitive changes (worker batching, GPU shader), include a
  short micro-benchmark or before/after throughput measurement in the PR
  description.

## Refactoring guidelines

- Prefer small, reversible refactors. Preserve public APIs in the crate.
- When optimizing hot paths, keep clarity: add comments explaining why a
  low-level optimization is required and include a benchmark when possible.
- Changes to `src/metal_gpu.rs` are high-risk: update shaders only after
  profiling and with a clear verification plan (unit tests cannot fully cover
  GPU numeric differences).

## Release & build

Build with `cargo build --release`. Run the shipped binary from
`target/release/meshcore-keygen` for performance testing.
