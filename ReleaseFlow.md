# Release/Build flow

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
