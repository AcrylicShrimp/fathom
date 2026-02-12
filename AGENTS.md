# Repository Agent Rules

## Rust Dependency Policy
- Before adding or updating Rust crates, check the latest published version with `cargo search <crate> --limit 1`.
- Use major-only version requirements for stable crates in `Cargo.toml` (examples: `1`, `4`).
- For pre-1.0 crates, specify major+minor requirements (examples: `0.14`, `0.3`, `0.29`), not bare `0`.
- After dependency changes, run `cargo update -w` and `cargo check --workspace`.

## Code Organization Policy
- Split implementation into manageable modules/files; avoid large blackhole files.
- Do not use `mod.rs`.
- Prefer module layout like:
  - `xxx.rs`
  - `xxx/abc.rs`
  - `xxx/def.rs`

## Post-Edit Verification
- After finishing code edits, always run:
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets`
