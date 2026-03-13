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

## Hard Cutover Architecture Policy
- Apply this section only when the user explicitly requests a hard-cutover architectural change or redesign. It does not apply to ordinary feature work or incremental maintenance.
- Do not treat existing patterns as mandatory. Replacing them with a stronger design is encouraged when the reasoning is clear and the new boundary is better.
- Do not preserve old code merely to keep the current system compiling or runnable during the transition. Temporary breakage is acceptable when landing a real architectural replacement.
- Do not add awkward compatibility bridges, adapter shims, or mixed old/new flows just to keep the replaced system alive. If the architecture is being replaced, do not optimize for compatibility.
- Assume forced compatibility usually weakens the redesign by reintroducing hard-codes, leaks, and legacy constraints into the new structure.
- Remove or destroy the replaced system, legacy setup paths, and outdated architecture when justified. Architectural redesign often requires follow-on changes across the surrounding system.
- Prefer strong abstraction boundaries so the new strategy, subsystem, or layer can stand on its own without reaching through the rest of the codebase.
- Make subsystems and layers self-contained. They should interact through dedicated interfaces rather than direct cross-system knowledge wherever possible.
- Do not directly modify unrelated systems to accommodate a redesign unless it is necessary and justified. Interface-level integration is preferred.

## Post-Edit Verification
- After finishing code edits, always run:
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets`
