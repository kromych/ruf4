# Release/Build flow

## Building from source

### Prerequisites

- [Rust](https://rustup.rs/) 1.96 or later (the workspace `rust-version`)

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

## Publishing to crates.io

All four workspace crates are published. The `stdext` and `lsh` names were
already taken on crates.io, so the packages are named `ruf4-stdext` and
`ruf4-lsh` while their libraries keep the short names (`[lib] name`), which is
why the source still says `use stdext::...` and `use lsh::...`.

| Package | Library | Contents |
|---------|---------|----------|
| `ruf4` | `ruf4` + `ruf4` binary | the file commander |
| `ruf4-tui` | `ruf4_tui` | TUI framework |
| `ruf4-lsh` | `lsh` | syntax highlighting compiler/runtime + `definitions/` |
| `ruf4-stdext` | `stdext` | arenas, collections, SIMD, Unicode |

The version lives once in `[workspace.package]` and every crate inherits it;
bump it there (and in `resources/windows/ruf4.rc`) before releasing.

Dry run the whole workspace first — this packages each crate and builds it from
its own tarball, with the sibling tarballs standing in for the registry:

```sh
cargo package --workspace
```

Then publish bottom-up, since each crate must already be on crates.io before
its dependents can be verified:

```sh
cargo publish -p ruf4-stdext
cargo publish -p ruf4-lsh
cargo publish -p ruf4-tui
cargo publish -p ruf4
```

Note that `ruf4`'s build script embeds the Windows icon and version resource
from `resources/windows/ruf4.rc`, which lives outside the package directory and
is therefore not part of the published tarball. Binaries built by `cargo install
ruf4` on Windows have no icon or version info; the GitHub release builds, which
build from a checkout, do.

## Syncing with Microsoft Edit

`stdext`, `tui` and `lsh` are derived from
[Microsoft Edit](https://github.com/microsoft/edit) and were last synced with
its commit `b7f8322` (2026-11-02). Upstream keeps the `stdext` and `tui` code
in its `edit` crate (`crates/edit/src`), while `lsh` is its own crate there too.
To sync again, 3-way merge upstream's changes since that commit into the
corresponding files here, formatting upstream's files with ruf4's default
`rustfmt` settings first, so that only real changes conflict.

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

Here's how re-create the tag and the release for a failed release flow:

```sh
gh release delete v0.0.1 --yes    # delete the GitHub release
git push origin :v0.0.1            # delete remote tag
git tag -f v0.0.1                  # re-create tag at HEAD
git push origin v0.0.1             # triggers fresh release workflow
```
