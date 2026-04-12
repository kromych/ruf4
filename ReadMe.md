# ruf4

[![CI](https://github.com/kromych/ruf4/actions/workflows/ci.yml/badge.svg)](https://github.com/kromych/ruf4/actions/workflows/ci.yml)

<p align="center">
  <img src="resources/icons/ruf4.svg" width="128" alt="ruf4 icon">
</p>

> NOTE: This is **alpha-quality** software! It _may_ delete and/or corrupt
> your files at some point and cause other losses, non-intentionally though.
> Also your sense of aesthetics _may_ be hurt. Use at _your own_ risk and expense.
> I'm always happy to make it better if you tell me what should be changed, or
> you can post a PR in the spirit of OSS.

We all sometimes file, and here is a double-panel file commander. It's been created
because I couldn't use what I wanted to due to legal regulations, and what I could
use looked bad, didn't have the true rough spirit of a double-panel file commander.
So rock ur files, folks :)

This is built in Rust on the TUI framework derived from
[Microsoft Edit](https://github.com/microsoft/edit). It is an immediate mode TUI,
very small and just enough.

Runs on Linux, macOS, and Windows.

## Keyboard shortcuts

### Navigation

| Key | Action |
|-----|--------|
| Up / Down | Move cursor |
| Page Up / Page Down | Scroll by page |
| Home / End | Jump to first / last entry |
| Enter | Enter directory or open file |
| Tab | Switch active panel |
| Backspace | Go to parent directory (in command line) |
| Alt+letters | Quick search: jump to file by name prefix |

### File operations

| Key | Action |
|-----|--------|
| F5 | Copy |
| F6 | Rename / Move |
| F7 | Make directory |
| F8 | Delete |
| Delete | Delete |
| Ctrl+G | Change root / drive |
| Ctrl+D | Directory history |
| Ctrl+E | Command history |
| Ctrl+R | Refresh both panels |

### Selection

| Key | Action |
|-----|--------|
| Insert / Shift+Space | Toggle selection on current entry |
| + | Select group (glob pattern) |
| - | Deselect group (glob pattern) |
| * | Invert selection |
| Ctrl+A | Select all |

### View & sorting

| Key | Action |
|-----|--------|
| Ctrl+Q | Toggle quick view panel |
| Ctrl+H | Toggle hidden files |
| Ctrl+F3 | Sort by name |
| Ctrl+F4 | Sort by extension |
| Ctrl+F5 | Sort by date |
| Ctrl+F6 | Sort by size |

### General

| Key | Action |
|-----|--------|
| F1 | Help screen |
| F2 | Save settings |
| F3 / Ctrl+Q | Toggle quick view panel |
| F9 | Focus menubar |
| F10 | Quit (with confirmation) |
| Any letter | Activate command line |

### Command line

Type any text to activate the command line at the bottom of the screen.
Commands run in the active panel's directory.

| Key | Action |
|-----|--------|
| Enter | Execute command |
| Escape | Cancel |
| Backspace | Delete character |

### Dialogs

Most confirmation dialogs respond to:

| Key | Action |
|-----|--------|
| Y / Enter | Confirm |
| N / Escape | Cancel |
| A | All (in overwrite prompts) |

### Mouse

- Click a panel to make it active
- Click a file entry to select it
- Double-click to enter a directory or open a file
- Scroll wheel to navigate
- Click the function key bar at the bottom for quick access

### Clicking around

| Area | Action |
|------|--------|
| Panel path (title bar) | Open change root dialog |
| File entry | Select entry; double-click to open/enter |
| Sort indicator in footer (`Sort:Name+`) | Open sort mode dialog |
| Hidden indicator in footer (`[H]` / `[ ]`) | Toggle hidden files |
| Function key bar (bottom row) | Invoke the corresponding F-key action |
| Help dialog entry | Close help and invoke the shortcut's action |

## Building from source

### Prerequisites

- [Rust](https://rustup.rs/) 1.93 or later

### Native build

```sh
cargo build --release
```

The binary is at `target/release/ruf4` (or `ruf4.exe` on Windows).

### Cross-compiling for Windows from macOS/Linux

Requires MinGW (`brew install mingw-w64` on macOS, `apt install mingw-w64` on Linux):

```sh
rustup target add x86_64-pc-windows-gnu
EDIT_CFG_ICUUC_SONAME="icu.dll" EDIT_CFG_ICUI18N_SONAME="icu.dll" \
    cargo build --release --target x86_64-pc-windows-gnu
```

### Cross-checking without a linker

You can type-check for any target without a linker installed:

```sh
rustup target add x86_64-pc-windows-msvc
EDIT_CFG_ICUUC_SONAME="icu.dll" EDIT_CFG_ICUI18N_SONAME="icu.dll" \
    cargo check --target x86_64-pc-windows-msvc
```

### macOS app bundle

After building a release binary:

```sh
python3 scripts/package-macos.py
```

This creates `target/release/ruf4.app` with the icon and Info.plist.

To sign and notarize (requires Apple Developer ID certificate):

```sh
export DEVELOPER_ID_APPLICATION="Developer ID Application: Your Name (TEAMID)"
export APPLE_TEAM_ID="XXXXXXXXXX"
export APPLE_ID="you@example.com"
export APPLE_ID_PASSWORD="xxxx-xxxx-xxxx-xxxx"  # app-specific password

python3 scripts/package-macos.py --sign --notarize
```

### Windows code signing

After building a release binary, sign with Azure Trusted Signing:

```sh
python3 scripts/sign-windows.py target\release\ruf4.exe
```

Requires `signtool.exe` (Windows SDK) and the Trusted Signing client:

```sh
dotnet tool install --global Microsoft.Trusted.Signing.Client
```

## Signing the releases

If you fork the project and want to sign the releases, configure these
GitHub repository secrets.

### macOS (Apple Developer ID)

| Secret | Description |
|--------|-------------|
| `APPLE_CERTIFICATE` | Base64-encoded .p12 Developer ID certificate |
| `APPLE_CERTIFICATE_PASSWORD` | Password for the .p12 file |
| `DEVELOPER_ID_APPLICATION` | Signing identity, e.g. `Developer ID Application: Name (TEAMID)` |
| `APPLE_TEAM_ID` | 10-character Apple Team ID |
| `APPLE_ID` | Apple ID email for notarization |
| `APPLE_ID_PASSWORD` | App-specific password (generate at [appleid.apple.com](https://appleid.apple.com)) |

### Windows (Azure Trusted Signing)

| Secret | Description |
|--------|-------------|
| `AZURE_TENANT_ID` | Azure AD tenant ID |
| `AZURE_CLIENT_ID` | App registration client ID |
| `AZURE_CLIENT_SECRET` | App registration client secret |
| `AZURE_ENDPOINT` | Trusted Signing account endpoint (e.g. `https://wus2.codesigning.azure.net`) |
| `AZURE_ACCOUNT_NAME` | Trusted Signing account name |
| `AZURE_CERT_PROFILE` | Certificate profile name |

Signing is skipped when secrets are not configured (PRs, forks without credentials).

## Supported platforms

| Platform | Architecture | Status |
|----------|-------------|--------|
| Linux | x86_64 | Supported |
| Linux | aarch64 | Supported |
| macOS | aarch64 (Apple Silicon) | Supported |
| Windows | x86_64 | Supported |
| Windows | aarch64 (ARM64) | Supported |

## CI / Releases

Pushing to `main` or opening a PR runs:
- `cargo fmt` format check
- `cargo clippy` lint checks
- Tests on Linux, macOS, and Windows
- Release builds on all platform/arch combinations (Windows binaries are Azure-signed when secrets are configured)

Pushing a version tag (e.g. `git tag v0.1.0 && git push --tags`) triggers a release workflow that:
- Builds release binaries for all platforms
- Signs and notarizes the macOS builds (if Apple credentials are configured)
- Packages as `.tar.gz` (Linux/macOS) or `.zip` (Windows)
- Creates a GitHub release with all archives

## License

MIT
