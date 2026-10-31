# Releasing

Every version that reaches `main` is released, and nothing else is. Releasing needs no tag pushed by hand: merging a pull request that raises the version is all it takes.

## Raising the version

The version is set once for the whole workspace, in `Cargo.toml`:

```sh
just bump patch    # 0.0.3 → 0.0.4; also minor, major, or a version such as 1.0.0-rc.1
```

`just bump` also updates `Cargo.lock`. Raising the version by hand leaves the lock behind, and CI fails with "cannot update the lock file … because --locked was passed".

Every pull request into `main` has to raise the version. The exception is a branch named `fix/*`: it merges without a release, and its changes go out with the next version that is raised.

`just version-check` runs the same check as CI, against `origin/main`.

## What a merge does

Each push to `main` runs the release workflow:

1. **Version:** if `v` plus the version from `Cargo.toml` is already a tag, there's nothing to do.
2. **Build:** the tests, the release build for x86-64-v2, and the package. The package holds the binary and the default settings as `config.toml`, and comes with a SHA-256 file.
3. **Publish:** a GitHub release with that tag, generated notes, the package and its checksum.

## The book

Every push to `main` also publishes this book, with the API documentation, at <https://tonigineer.github.io/tonish/>. This happens whether or not the version changed, so a fix to the book from a `fix/*` branch goes online right away. The workflow is `.github/workflows/book.yml`, and you can also start it by hand from the Actions tab.
