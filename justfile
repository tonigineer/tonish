set shell := ["bash", "-euo", "pipefail", "-c"]

just := just_executable()

# `grep .` fails on empty input, so that a missing version stops a recipe
# instead of comparing as an empty string.
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

# A copy of the release build rather than `cargo install`, so that installing
# right after `just desktop` compiles nothing, and the binary goes to
# ~/.local/bin, the user's place for programs in the XDG layout, instead of
# ~/.cargo/bin.
[doc("Build tonish for this machine and install it to DIR, or to ~/.local/bin if not given.")]
[group("develop")]
install dir="":
    cargo build --release --locked -p tonish
    install -Dm755 target/release/tonish "{{ if dir == "" { home_directory() / ".local" / "bin" } else { dir } }}/tonish"

[doc("Format all the code: Rust with rustfmt, Python with ruff.")]
[group("develop")]
fmt:
    cargo fmt --all
    uv run ruff format

[doc("Remove what building and testing left.")]
[group("develop")]
clean:
    cargo clean
    rm -rf tests/desktop/out book/build .venv .pytest_cache .ruff_cache

# ── Check ─────────────────────────────────────────────────────────────────────

[doc("Check the formatting, and lint with clippy and ruff, with warnings as errors.")]
[group("check")]
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --release -- -D warnings
    uv run ruff format --check
    uv run ruff check

[doc("Run the unit tests with nextest, then the doctests.")]
[group("check")]
test:
    cargo nextest run --workspace --release --no-tests=pass
    cargo test --workspace --release --doc

[doc("Lint, then test: the code checks of a pull request, short of the desktop tests.")]
[group("check")]
check: lint test

[doc("Run the desktop tests: tonish on a headless sway. ARGS go to pytest, such as -k redraw or --lf. Needs sway, grim, Mesa and uv, and wtype for the tests that type keys.")]
[group("check")]
[positional-arguments]
desktop *args:
    cargo build --release -p tonish
    uv run pytest "$@"

[doc("All the code checks of a pull request: check, then the desktop tests.")]
[group("check")]
check-all: check desktop

# ── Bench ─────────────────────────────────────────────────────────────────────

[doc("Benchmark drawing on the CPU and the GPU, only the cases whose name holds FILTER if given, such as cpu or gpu.")]
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

# ── Docs ──────────────────────────────────────────────────────────────────────

# Private items too, because most of the engine is private and that is the
# part a developer reads the API documentation for.
[doc("Build the API documentation into target/doc, with warnings as errors. --open shows it.")]
[group("docs")]
doc *args:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items {{ args }}

[doc("Build the book, with the API documentation under api/, into book/build.")]
[group("docs")]
book: doc
    mdbook build book
    rm -rf book/build/api
    cp -r target/doc book/build/api

[doc("Serve the book at http://localhost:3000 and rebuild it on every change, without the API documentation.")]
[group("docs")]
book-serve:
    mdbook serve book --open

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

# sort -V orders plain X.Y.Z correctly, but would put a pre-release such as
# 1.0.0-rc.1 after 1.0.0.
[private]
[no-exit-message]
_newer old new:
    @[ "{{ old }}" != "{{ new }}" ] && [ "$(printf '%s\n' {{ old }} {{ new }} | sort -V | tail -n 1)" = "{{ new }}" ] \
        || { echo "{{ new }} is not newer than {{ old }}" >&2; exit 1; }
    @echo "{{ old }} -> {{ new }}"
