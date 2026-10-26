set shell := ["bash", "-euo", "pipefail", "-c"]

[doc("List the commands, by group.")]
default:
    @just --list

# ── Checks ──────────────────────────────────────────────────────────────────

[doc("Format all the code with rustfmt.")]
[group("check")]
fmt:
    cargo fmt --all

[doc("What CI checks: formatting, clippy with warnings as errors, and every test.")]
[group("check")]
check: && test
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --release -- -D warnings

# ── Build ───────────────────────────────────────────────────────────

[doc("Build the whole workspace.")]
[group("build")]
build args="":
    cargo build {{ args }}

[group("build")]
[doc("Run tonish.")]
run args="":
    cargo run --release -- {{ args }}

# ── Test ──────────────────────────────────────────────────────────

[doc("Run the unit tests with nextest, then the doctests.")]
[group("test")]
test:
    cargo nextest run --workspace --release --no-tests=pass
    cargo test --workspace --release --doc

# [doc("Run one crate's tests, only those whose name holds FILTER if given.")]
# [group("test")]
# unit crate filter="":
#     cargo nextest run --release -p {{ crate }} --no-tests=pass {{ filter }}

# [doc("Remove what building and testing left.")]
# [group("release")]
# clean:
#     cargo clean
