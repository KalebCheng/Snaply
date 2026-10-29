# Snaply

Snaply is a fast, native screenshot and annotation app for macOS, Windows, and Linux. Capture a region or window, annotate it in place, then copy or save without leaving the screen.

Built with Tauri 2, React, TypeScript, and Rust. Screen capture is powered by [xcap](https://github.com/nashaofu/xcap); annotation runs on a custom Canvas editor.

[![CI](https://github.com/KalebCheng/Snaply/actions/workflows/ci.yml/badge.svg)](https://github.com/KalebCheng/Snaply/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/KalebCheng/Snaply?include_prereleases&sort=semver)](https://github.com/KalebCheng/Snaply/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Download

Download the latest installer from [GitHub Releases](https://github.com/KalebCheng/Snaply/releases).

| Platform | Packages |
| --- | --- |
| macOS 10.13+ | `.dmg` for Apple Silicon and Intel |
| Windows 10+ | `.exe` (NSIS) and `.msi` |
| Linux | `.AppImage`, `.deb`, and `.rpm` |

> macOS requires Screen Recording permission. Enable Snaply in **System Settings → Privacy & Security → Screen Recording**, then restart the app.

Current community builds are unsigned. macOS may require **Open Anyway**, and Windows SmartScreen may show a warning.

## Features

- **One shortcut to capture** — global shortcut, configurable from Settings; Snaply starts silently in the system tray.
- **Window and region capture** — live window highlighting on macOS, menu bar and desktop fallbacks, precise region selection with resize handles, and a pixel magnifier with a built-in color picker.
- **In-place annotation** — rectangle, ellipse, arrow, pen, mosaic, text, watermark, and crop. Move, undo, redo, or delete annotations before exporting.
- **Fast handoff** — copy the annotated result, save it as PNG, or press `Enter` to finish.
- **Continue from anywhere** — drag an image into the editor or paste an image from the clipboard.
- **Native updates** — check for the latest GitHub release from the About pane.

## Keyboard Shortcuts

| Action | Shortcut |
| --- | --- |
| Start capture | `⌘⇧1` / `Ctrl+Shift+1` |
| Open settings | `⌘,` / `Ctrl+,` |
| Copy / save | `⌘C` / `⌘S` (`Ctrl+C` / `Ctrl+S`) |
| Undo / redo | `⌘Z` / `⇧⌘Z` (`Ctrl+Z` / `Ctrl+Shift+Z`) |
| Select / rectangle / ellipse / arrow | `V` / `R` / `O` / `A` |
| Pen / mosaic / text / watermark / crop | `P` / `B` / `T` / `W` / `X` |
| Toggle magnifier | `M` |
| Copy color under cursor | `⌘C` / `Ctrl+C` in capture mode |
| Switch color format | `Shift` |
| Zoom / pan canvas | `⌘` + wheel / middle mouse or `Alt` + drag |
| Cancel | `Esc` or right-click |

## Development

### Requirements

- Node.js 20+
- pnpm 10+
- Rust 1.80+
- Platform dependencies from the [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/)

On Ubuntu/Debian, install the Tauri system packages:

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
  libx11-dev libxcb1-dev libdbus-1-dev libpipewire-0.3-dev libwayland-dev
```

Install dependencies and start the desktop app:

```bash
pnpm install
pnpm tauri dev
```

Run the same checks used by CI:

```bash
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
```

Build a local installer:

```bash
pnpm tauri build
```

Bundles are written to `src-tauri/target/release/bundle/`.

## Project Structure

```text
src/
  overlay/      Capture overlay: selection, window detection, magnifier
  editor/       Annotation editor and inspector
  settings/     Settings and about windows
  lib/          IPC, rendering, geometry, and export helpers
src-tauri/src/
  overlay.rs    Frame capture, overlay lifecycle, crop and save
  platform.rs   Platform-specific permissions, DPI, coordinates, and windows
  tray.rs       Tray menu and global shortcuts
  settings.rs   Persistent user settings
  updates.rs    GitHub release update check
```

Annotations are stored as objects over an immutable base capture. Undo is a state pop, and export redraws at the original physical resolution. Coordinates are converted explicitly between logical window/selection space and physical screenshot pixels for high-DPI displays.

## Platform Support

| | macOS | Windows | Linux |
| --- | --- | --- | --- |
| Screen capture | Screen Recording permission | Yes | X11 / XWayland |
| Window detection | Native z-order | Geometric hit testing | Window-manager dependent |
| Global shortcut | Yes | Yes | May be unavailable on Wayland; tray works |
| Release bundles | `.dmg` (arm64, x64) | `.msi`, NSIS `.exe` | `.AppImage`, `.deb`, `.rpm` |

Pure Wayland sessions cannot be captured directly due to an `xcap` limitation. Snaply reports a clear error and asks you to use X11 or XWayland.

## Releases

Releases are cut from semantic version tags on the `main` branch:

```text
0.0.1-rc.1
0.0.1
0.1.0
1.0.0
```

Before tagging, update the version in all three files:

```text
package.json
src-tauri/Cargo.toml
src-tauri/tauri.conf.json
```

The tag must match those versions. Tags with a prerelease suffix such as `-rc.1` are published as GitHub prereleases; final versions are published as stable releases.

```bash
git tag 0.0.1-rc.1
git push origin 0.0.1-rc.1
```

Pushing the tag builds macOS, Windows, and Linux packages in parallel and creates the GitHub Release with all installers attached.

## Contributing

Contributions are welcome. For anything larger than a typo or small bug fix, open an issue first so the approach can be agreed before you invest time in a pull request.

1. Fork the repository and create a branch from `main`.
2. Keep changes focused and avoid unrelated refactors.
3. Follow the existing code style. Run the CI checks locally.
4. Add or update tests when behavior changes.
5. Write commit messages in [Conventional Commits](https://www.conventionalcommits.org/) format, for example `fix(overlay): keep selection inside monitor bounds`.
6. Open a pull request against `main` and describe the problem, the solution, and how it was tested.

### Pull Request Checklist

- [ ] The change is scoped to one problem.
- [ ] `pnpm build`, `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, and `cargo test --manifest-path src-tauri/Cargo.toml --all-targets` pass.
- [ ] New behavior is covered by a test, or the PR explains why it cannot be.
- [ ] User-visible changes include screenshots or recordings.
- [ ] Platform-specific behavior was tested and the tested platforms are listed.
- [ ] Documentation is updated when setup, shortcuts, or behavior changes.
- [ ] No credentials, generated installers, or unrelated formatting changes are included.

PRs must pass CI before review. Maintainers may ask for a smaller scoped change when a PR mixes unrelated work.

### Reporting Bugs

Open an [issue](https://github.com/KalebCheng/Snaply/issues) with:

- Snaply version, operating system, and display server (X11/Wayland on Linux)
- Steps to reproduce
- Expected and actual behavior
- Screenshots or logs when available

For security vulnerabilities, do not open a public issue. Contact the maintainer privately through the GitHub profile linked in the repository.

## License

Snaply is released under the [MIT License](LICENSE).

## Acknowledgements

Snaply builds on [Tauri](https://tauri.app/), [xcap](https://github.com/nashaofu/xcap), [React](https://react.dev/), and [Vite](https://vite.dev/).
