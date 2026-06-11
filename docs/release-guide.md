# Version Release Guide

## Semantic Versioning (SemVer)

| Change Type | Version | Example | Description |
|-------------|---------|---------|-------------|
| Bug fix | PATCH | 0.1.4 → 0.1.5 | Backward-compatible fixes |
| New feature | MINOR | 0.1.5 → 0.2.0 | Backward-compatible additions |
| Breaking change | MAJOR | 0.x.x → 1.0.0 | Incompatible API changes |

## Pre-1.0 Convention

While version is `0.x.x`:
- `0.1.x` → patch fixes
- `0.x.0` → new features or breaking changes (since no stable API yet)

## Release Workflow

```bash
# 1. Update version in Cargo.toml and crates/server/Cargo.toml
# 2. Commit
git commit -m "chore: bump version to X.Y.Z"

# 3. Tag and push (GitHub Actions will create draft release automatically)
git tag -a vX.Y.Z -m "vX.Y.Z: brief description"
git push origin vX.Y.Z
```

**DO NOT** manually create release with `gh release create`. Let CI handle it.

## Checklist Before Release

- [ ] All tests pass: `cargo test --workspace`
- [ ] Version updated in both `Cargo.toml` and `crates/server/Cargo.toml`
- [ ] Changelog reflects all changes since last release
- [ ] Bug fixes include regression test if applicable
