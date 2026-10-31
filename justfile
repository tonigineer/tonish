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

[group("run")]
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

# ── Bench ─────────────────────────────────────────────────────────

[doc("Benchmark the CPU rasteriser, only the cases whose name holds FILTER if given.")]
[group("bench")]
bench filter="":
    cargo bench -p engine --bench draw -- {{ filter }}

[doc("Benchmark and keep the results as baseline NAME, to measure a change against.")]
[group("bench")]
bench-save name="before":
    cargo bench -p engine --bench draw -- --save-baseline {{ name }}

[doc("Benchmark and compare with baseline NAME, saved earlier with bench-save.")]
[group("bench")]
bench-compare name="before":
    cargo bench -p engine --bench draw -- --baseline {{ name }}

# ── Release ───────────────────────────────────────────────────────

[doc("Print the workspace version, or the one committed at REF if given.")]
[group("release")]
version ref="":
    @{{ if ref == "" { "cat Cargo.toml" } else { "git show " + ref + ":Cargo.toml" } }} | sed -n '/^\[workspace\.package\]/,/^\[/s/^version *= *"\([^"]*\)".*/\1/p'

[doc("Raise the version (LEVEL: major, minor, patch, or X.Y.Z) and update Cargo.lock to match.")]
[group("release")]
bump level:
    #!/usr/bin/env bash
    set -euo pipefail
    old=$({{ just_executable() }} version)
    IFS=. read -r major minor patch <<< "${old%%[-+]*}"
    case "{{ level }}" in
        major) new="$((major + 1)).0.0" ;;
        minor) new="$major.$((minor + 1)).0" ;;
        patch) new="$major.$minor.$((patch + 1))" ;;
        *) new="{{ level }}" ;;
    esac
    if [ "$new" = "$old" ] || [ "$(printf '%s\n' "$old" "$new" | sort -V | tail -n 1)" != "$new" ]; then
        echo "$new is not newer than $old" >&2
        exit 1
    fi
    sed -i "/^\[workspace\.package\]/,/^\[/s/^version *= *\"[^\"]*\"/version = \"$new\"/" Cargo.toml
    cargo update --workspace
    echo "$old -> $new"

[doc("Check the version is newer than on REF, as pull requests into main must be.")]
[group("release")]
version-check ref="origin/main":
    #!/usr/bin/env bash
    set -euo pipefail
    old=$({{ just_executable() }} version {{ ref }})
    new=$({{ just_executable() }} version)
    if [ "$new" = "$old" ] || [ "$(printf '%s\n' "$old" "$new" | sort -V | tail -n 1)" != "$new" ]; then
        echo "version $new is not newer than $old on {{ ref }}" >&2
        exit 1
    fi
    echo "version $old -> $new"
