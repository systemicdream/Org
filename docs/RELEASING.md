# Releasing Org

Org follows [Semantic Versioning](https://semver.org/). While it's in early development the
version stays at **0.x**: bump the minor version (0.1 → 0.2) for new features or anything that
changes how Org behaves, and the patch version (0.1.0 → 0.1.1) for fixes only. 1.0 comes once
Org has been used in the wild and its file handling is trusted.

The version lives in one place: `version` under `[workspace.package]` in `Cargo.toml`. Every
crate, the macOS bundle, the Linux packages and Settings → About read it from there.

## Steps

1. **Pick the version** and update `Cargo.toml` (then `cargo check` so `Cargo.lock` follows).
2. **CHANGELOG.md:** move the entries from `[Unreleased]` into a new
   `## [X.Y.Z] - YYYY-MM-DD` section, written for people using Org, not developers. Update
   the comparison links at the bottom.
3. **AppStream metainfo** (`packaging/linux/io.github.systemicdream.Org.metainfo.xml`): add
   a `<release version="X.Y.Z" date="YYYY-MM-DD">` at the top of `<releases>`.
4. **Merge** these changes to `main` through a pull request, as usual.
5. **Tag** the merge commit and push the tag:

   ```sh
   git checkout main && git pull
   git tag -a vX.Y.Z -m "Org X.Y.Z"
   git push origin vX.Y.Z
   ```

6. The **Release** workflow builds the macOS `.dmg`, the Flatpak bundle, the `.deb`, `.rpm`
   and tarball, checks that the tag matches `Cargo.toml` and that the changelog has a dated
   entry, and drafts a GitHub Release with the changelog entry, install instructions
   (`docs/release-footer.md`) and `SHA256SUMS`.
7. **Try the packages** from the draft (at least the `.dmg` and the Flatpak), then
   **publish** the draft on GitHub. Don't mark it as a pre-release: Org's update check asks
   GitHub for the latest release, which skips pre-releases.

To try the packages without releasing, run the workflow by hand (Actions → Release → Run
workflow): it builds everything as downloadable artifacts and stops there.

## Building packages locally

| Package | Command | Needs |
|---|---|---|
| macOS `.app` and `.dmg` | `scripts/bundle-macos.sh --universal --dmg` | `rustup target add x86_64-apple-darwin` |
| Flatpak bundle | `scripts/package-flatpak.sh` | `flatpak`, `flatpak-builder` |
| `.deb`, `.rpm`, tarball | `scripts/package-linux.sh` | `dpkg-deb`, `rpmbuild` |

Everything lands in `target/dist/`.

## Signing

- **macOS:** builds are ad-hoc signed, not signed with a Developer ID or notarized (that
  needs a paid Apple Developer account). People have to approve Org once in System
  Settings → Privacy & Security. Signing and notarizing can be added to the workflow later
  with the account's certificate and an App Store Connect API key as repository secrets.
- **Linux:** packages aren't signed; `SHA256SUMS` lets people check their downloads. The
  Flatpak isn't on Flathub yet: publishing there would need vendored cargo sources (the
  manifest currently downloads crates at build time).
