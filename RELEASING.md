# Releasing HeadroomLab

HeadroomLab updates itself from **GitHub Releases**. On launch it asks
`api.github.com/repos/<owner>/<repo>/releases/latest` for the newest release,
looks for the one asset that matches this machine, downloads it and installs it.

There is no CI yet, so releases are cut by hand. This document is the contract
between what you upload and what the updater will accept.

---

## The naming contract

**Get the asset name wrong and the updater silently reports "up to date".** That
is by design — a release with nothing for your platform is not something a user
can act on — but it does mean a typo here is quiet rather than loud.

Assets must be named:

```
HeadroomLab-<version>-<target>.<ext>
```

| | `<target>` | `<ext>` |
|---|---|---|
| macOS (Apple Silicon) | `macos-aarch64` | `.tar.gz` |
| macOS (Intel) | `macos-x86_64` | `.tar.gz` |
| Linux | `linux-x86_64` / `linux-aarch64` | `.tar.gz` |
| Windows | `windows-x86_64` / `windows-aarch64` | `.zip` |

- `<version>` carries **no** leading `v` — `HeadroomLab-0.2.0-macos-aarch64.tar.gz`.
- The **git tag** does: `v0.2.0`. The updater strips it when comparing.
- The version must be **strictly greater** than the running one or nothing
  happens. Downgrades and re-releases of the same version are ignored.
- Extra assets (checksums, source archives) are ignored — only a name matching
  the pattern *and* ending in the right extension is considered.

The pattern itself is configurable at build time via `HL_UPDATE_ASSET_PATTERN`
(default `HeadroomLab-{version}-{target}`); see `src/config.rs`.

### What each archive must contain

| Platform | Contents |
|---|---|
| macOS | `HeadroomLab.app` at the archive root |
| Linux | `HeadroomLab` (the executable) |
| Windows | `HeadroomLab.exe` |

One level of nesting is tolerated — an archive made from a containing folder
works — but the root is what the updater looks at first.

---

## Cutting a release

### 1. Bump the version

Edit `version` in `Cargo.toml`, then `cargo build` so `Cargo.lock` follows.
Commit.

### 2. Build each platform's artifact

Each has to be built **on** the platform it targets; there is no cross-compiling
set up.

**macOS**

```sh
./packaging/macos/bundle.sh
```

Produces `target/release/bundle/HeadroomLab-<version>-macos-<arch>.tar.gz`. The
script fills `CFBundleShortVersionString` from `Cargo.toml`, so the version macOS
reports cannot drift from the one the updater compares, and copies
`packaging/macos/AppIcon.icns` into the bundle.

**Linux**

```sh
./packaging/linux/bundle.sh
```

Produces `target/release/bundle/HeadroomLab-<version>-linux-<arch>.tar.gz`
containing the binary, the icon theme, the desktop entry and `install.sh`.

**Windows**

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\bundle.ps1
```

Produces `target\release\bundle\HeadroomLab-<version>-windows-<arch>.zip`. The
icon is *inside* the `.exe` — `build.rs` embeds it — so the archive needs nothing
but the executable.

All three scripts set `HL_UPDATE_ENABLED=1` explicitly. Release builds default to
updates-on, but this is the artifact users run, and it is the one build that must
be able to replace itself.

### 3. Tag and publish

```sh
git tag v<version>
git push origin v<version>
```

Create the release on GitHub against that tag and attach every artifact. It must
**not** be marked as a pre-release — the updater reads `/releases/latest`, which
excludes those.

### 4. Verify

From a copy of the *previous* version:

```sh
open /Applications/HeadroomLab.app
```

The splash should show "Downloading v<version>…" with a filling bar, then
"Installing…", then relaunch. Confirm the version at the bottom of the splash
has changed.

---

## Testing the updater without cutting a release

Updates are **off in debug builds** so `cargo run` never reaches the network.
To exercise the flow:

```sh
# Point at any repo that has releases, and name an asset that exists there.
HL_UPDATE_ENABLED=1 \
HL_UPDATE_REPO_OWNER=jaemk \
HL_UPDATE_REPO_NAME=self_update \
HL_UPDATE_ASSET_PATTERN='self_update-v{version}-aarch64-apple-darwin' \
cargo run
```

This downloads a real asset and extracts it, then refuses to install because the
binary is not inside a `HeadroomLab.app` — which is the safe way to exercise
download, progress and extraction without replacing anything.

`build.rs` declares each `HL_*` variable, so changing one triggers a rebuild
rather than silently reusing a cached binary.

---

## Icons

Every platform's icon is generated from one renderer, so they cannot drift from
each other or from the mark the app draws at runtime:

```sh
python3 packaging/make_icons.py
```

That writes, and you commit:

| File | Used by |
|---|---|
| `packaging/icon.png` | the 1024 master, for reference |
| `packaging/icon-256.png` | **embedded in the binary** (`main.rs`) as the window icon |
| `packaging/macos/AppIcon.icns` | the `.app` bundle, via `CFBundleIconFile` |
| `packaging/windows/AppIcon.ico` | **embedded in the `.exe`** by `build.rs` |
| `packaging/linux/hicolor/<size>/apps/headroomlab.png` | the freedesktop icon theme |

These are committed artifacts. The generator is not run at build time — nothing
in the build depends on Python.

### Two different icons

Worth keeping straight, because supplying one does not supply the other:

- The **launcher icon** — Finder, the Dock, Explorer, the application menu. It
  comes from the `.app` bundle (macOS), a resource inside the `.exe` (Windows),
  or the `.desktop` entry (Linux).
- The **window icon** — the title bar, the taskbar button, alt-tab. It is set at
  runtime from `packaging/icon-256.png` via `ViewportBuilder::with_icon`.
  **macOS ignores it** and uses the bundle icon for both.

So Windows and Linux need both; macOS needs only the `.icns`.

### Replacing the artwork

Drop your own files at the paths in the table above and stop running the
generator — nothing reads it. Keep `icon-256.png` as **straight RGBA8**; the
decoder rejects anything else and falls back to no window icon rather than
rendering garbage, and `cargo test --bin HeadroomLab` asserts the committed file
still decodes.

### Per-platform notes

**macOS.** `bundle.sh` warns and continues if the `.icns` is missing, so a
release is never blocked over a cosmetic file — watch for
`note: packaging/macos/AppIcon.icns not found` in its output. Finder aggressively
caches app icons; if a rebuilt bundle still shows the old one, that is the cache,
not the build:

```sh
touch target/release/bundle/HeadroomLab.app
killall Finder Dock
```

**Windows.** The icon is compiled in by `build.rs` using `winresource`, which
needs `rc.exe` (Visual Studio Build Tools) or `windres` (the GNU toolchain) on
the build machine. Without one the build still succeeds and prints a
`cargo::warning` — the executable simply gets the generic icon. Explorer caches
icons per-path, so test a fresh copy rather than one you have already opened.

**Linux.** Icons only appear once they are in the icon theme, which is what
`install.sh` does; running the binary straight out of the tarball shows the
window icon but no launcher entry. `StartupWMClass=HeadroomLab` in
`headroomlab.desktop` is what lets the running window match its launcher entry —
without it the taskbar shows a second, unnamed entry. The updater replaces only
the *binary*, so icons and the desktop entry survive updates untouched.

---

## Signing and notarisation

**Not currently done.** Until it is, a macOS user who downloads a release in a
browser gets *"HeadroomLab is damaged and can't be opened"* — Gatekeeper
refusing an unsigned, un-notarised app that carries a quarantine attribute.

They can get past it with:

```sh
xattr -dr com.apple.quarantine /Applications/HeadroomLab.app
```

or by right-clicking the app and choosing **Open** the first time. Say so in the
release notes.

**Apps updated in place are unaffected** — the updater extracts the new bundle
itself, and nothing in that path sets a quarantine flag. Only the first install
is impacted.

### When you have certificates

With an Apple Developer account (a paid membership is required for
notarisation), insert this into `bundle.sh` between assembling the bundle and
packing the tarball, replacing the ad-hoc `codesign` call:

```sh
# 1. Sign, with a hardened runtime — notarisation refuses anything without it.
codesign --force --deep --options runtime --timestamp \
  --sign "Developer ID Application: Your Name (TEAMID)" \
  target/release/bundle/HeadroomLab.app

# 2. Notarisation takes a zip, not a tarball.
ditto -c -k --keepParent \
  target/release/bundle/HeadroomLab.app /tmp/HeadroomLab-notarize.zip

# 3. Submit and wait. Store credentials once with:
#    xcrun notarytool store-credentials "HL_NOTARY" \
#      --apple-id you@example.com --team-id TEAMID --password <app-specific-password>
xcrun notarytool submit /tmp/HeadroomLab-notarize.zip \
  --keychain-profile "HL_NOTARY" --wait

# 4. Staple the ticket so the app validates offline.
xcrun stapler staple target/release/bundle/HeadroomLab.app

# 5. Verify before shipping.
codesign --verify --deep --strict --verbose=2 target/release/bundle/HeadroomLab.app
spctl --assess --type execute --verbose target/release/bundle/HeadroomLab.app
```

Only then pack the tarball — the archive must contain the **stapled** bundle.

Two things worth knowing:

- **Staple, don't just notarise.** Without `stapler staple` the app needs network
  access on first launch to validate, which fails offline.
- **Sign after any modification.** The updater replaces the whole bundle
  directory rather than the inner binary precisely so a signed bundle stays
  internally consistent; swapping the executable alone would break its signature.

Windows has the same story with `signtool` and an EV certificate; without one,
SmartScreen warns on first run. Not currently done either.
