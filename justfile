set shell := ["bash", "-euo", "pipefail", "-c"]

just := just_executable()

# The workspace version, from Cargo.toml on stdin; fails if there is none.
read_version := "sed -n 's/^version *= *\"\\(.*\\)\"/\\1/p' | grep ."

[doc("List the commands, by group.")]
default:
    @just --list --unsorted

# ── Develop ───────────────────────────────────────────────────────────────────

[doc("Run tonish.")]
[group("develop")]
run *args:
    cargo run --release -- {{ args }}

[doc("Build the whole workspace.")]
[group("develop")]
build *args:
    cargo build --workspace {{ args }}

[doc("Format all the code with rustfmt.")]
[group("develop")]
fmt:
    cargo fmt --all

[doc("Remove what building and testing left.")]
[group("develop")]
clean:
    cargo clean
    rm -rf tests/desktop/out

# ── Check ─────────────────────────────────────────────────────────────────────

[doc("Check the formatting, and run clippy with warnings as errors.")]
[group("check")]
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --release -- -D warnings

[doc("Run the unit tests with nextest, then the doctests.")]
[group("check")]
test:
    cargo nextest run --workspace --release --no-tests=pass
    cargo test --workspace --release --doc

[doc("Lint, then test: what a pull request checks, short of the desktop scenarios.")]
[group("check")]
check: lint test

[doc("Run the desktop scenarios: tonish on a headless sway. FILTER: part of a scenario's name. Needs sway and grim.")]
[group("check")]
desktop filter="":
    cargo build --release -p tonish
    python3 tests/desktop/run.py {{ filter }}

[doc("Everything a pull request checks: check, then the desktop scenarios.")]
[group("check")]
check-all: check desktop

# ── Bench ─────────────────────────────────────────────────────────────────────

[doc("Benchmark the CPU rasteriser, only the cases whose name holds FILTER if given.")]
[group("bench")]
bench filter="":
    cargo bench -p renderer --bench draw -- {{ filter }}

[doc("Benchmark and keep the results as baseline NAME, to measure a change against.")]
[group("bench")]
bench-save name="before":
    cargo bench -p renderer --bench draw -- --save-baseline {{ name }}

[doc("Benchmark and compare with baseline NAME, saved earlier with bench-save.")]
[group("bench")]
bench-compare name="before":
    cargo bench -p renderer --bench draw -- --baseline {{ name }}

# ── Release ───────────────────────────────────────────────────────────────────

[doc("Print the workspace version, or the one committed at REF if given.")]
[group("release")]
version ref="":
    @{{ if ref == "" { "cat Cargo.toml" } else { "git show " + ref + ":Cargo.toml" } }} | {{ read_version }}

[doc("Raise the version (LEVEL: major, minor, patch, or X.Y.Z) and update Cargo.lock to match.")]
[group("release")]
[no-exit-message]
[script("bash", "-euo", "pipefail")]
bump level:
    old=$({{ just }} version)
    IFS=. read -r x y z <<< "${old%%[-+]*}"
    case {{ level }} in
        major) new=$((x + 1)).0.0 ;;
        minor) new=$x.$((y + 1)).0 ;;
        patch) new=$x.$y.$((z + 1)) ;;
        *) new={{ level }} ;;
    esac
    {{ just }} _newer "$old" "$new"
    sed -i "s/^version *= *\".*\"/version = \"$new\"/" Cargo.toml
    cargo update --workspace --quiet

[doc("Check the version is newer than on REF, as pull requests into main must be.")]
[group("release")]
[no-exit-message]
version-check ref="origin/main":
    @old=$({{ just }} version {{ ref }}) && new=$({{ just }} version) && {{ just }} _newer "$old" "$new"

# Fails unless NEW is a later version than OLD. sort -V orders plain X.Y.Z
# correctly, but would put a pre-release such as 1.0.0-rc.1 after 1.0.0.
[private]
[no-exit-message]
_newer old new:
    @[ "{{ old }}" != "{{ new }}" ] && [ "$(printf '%s\n' {{ old }} {{ new }} | sort -V | tail -n 1)" = "{{ new }}" ] \
        || { echo "{{ new }} is not newer than {{ old }}" >&2; exit 1; }
    @echo "{{ old }} -> {{ new }}"
