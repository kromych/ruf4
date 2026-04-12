#!/usr/bin/env python3
"""Package ruf4 as a macOS .app bundle and optionally sign + notarize.

Usage:
    python3 scripts/package-macos.py [--sign] [--notarize]

Environment variables (required for --sign / --notarize):
    DEVELOPER_ID_APPLICATION  - Signing identity, e.g. "Developer ID Application: Name (TEAMID)"
    APPLE_TEAM_ID             - 10-char Apple Team ID
    APPLE_ID                  - Apple ID email for notarization
    APPLE_ID_PASSWORD         - App-specific password (NOT your account password)

The script expects the release binary at target/release/ruf4
(or a path set via BINARY_PATH).
"""

import argparse
import os
import re
import shutil
import stat
import subprocess
import sys
from pathlib import Path


def run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    print(f"    $ {' '.join(cmd)}")
    return subprocess.run(cmd, check=True, **kwargs)


def read_version(root: Path) -> str:
    cargo_toml = root / "crates" / "ruf4" / "Cargo.toml"
    text = cargo_toml.read_text()
    m = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
    if not m:
        sys.exit("Error: could not read version from Cargo.toml")
    return m.group(1)


def build_bundle(root: Path, binary: Path, version: str) -> Path:
    app_dir = root / "target" / "release" / "ruf4.app"

    # Clean previous bundle.
    if app_dir.exists():
        shutil.rmtree(app_dir)

    # Create .app structure.
    (app_dir / "Contents" / "MacOS").mkdir(parents=True)
    (app_dir / "Contents" / "Resources").mkdir(parents=True)

    # Copy binary.
    dest_bin = app_dir / "Contents" / "MacOS" / "ruf4"
    shutil.copy2(binary, dest_bin)
    dest_bin.chmod(dest_bin.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

    # Copy Info.plist with version substitution.
    plist_src = root / "resources" / "macos" / "Info.plist"
    plist_text = plist_src.read_text().replace("0.1.0", version)
    (app_dir / "Contents" / "Info.plist").write_text(plist_text)

    # Copy icon.
    shutil.copy2(root / "resources" / "icons" / "ruf4.icns", app_dir / "Contents" / "Resources" / "ruf4.icns")

    # Write PkgInfo.
    (app_dir / "Contents" / "PkgInfo").write_text("APPL????")

    return app_dir


def sign(root: Path, app_dir: Path) -> None:
    identity = os.environ.get("DEVELOPER_ID_APPLICATION")
    if not identity:
        sys.exit("Error: DEVELOPER_ID_APPLICATION not set")

    entitlements = root / "resources" / "macos" / "entitlements.plist"

    print(f"==> Signing with identity: {identity}")
    run([
        "codesign", "--force", "--deep", "--options", "runtime",
        "--entitlements", str(entitlements),
        "--sign", identity,
        "--timestamp",
        str(app_dir),
    ])

    print("==> Verifying signature")
    run(["codesign", "--verify", "--deep", "--strict", "--verbose=2", str(app_dir)])
    print("==> Signature OK")


def notarize(root: Path, app_dir: Path) -> None:
    apple_id = os.environ.get("APPLE_ID")
    password = os.environ.get("APPLE_ID_PASSWORD")
    team_id = os.environ.get("APPLE_TEAM_ID")

    if not all([apple_id, password, team_id]):
        sys.exit("Error: APPLE_ID, APPLE_ID_PASSWORD, and APPLE_TEAM_ID must be set")

    zip_path = root / "target" / "release" / "ruf4-macos-notarize.zip"

    print("==> Creating zip for notarization")
    run(["ditto", "-c", "-k", "--keepParent", str(app_dir), str(zip_path)])

    print("==> Submitting for notarization")
    run([
        "xcrun", "notarytool", "submit", str(zip_path),
        "--apple-id", apple_id,
        "--password", password,
        "--team-id", team_id,
        "--wait",
    ])

    print("==> Stapling notarization ticket")
    run(["xcrun", "stapler", "staple", str(app_dir)])

    zip_path.unlink(missing_ok=True)
    print("==> Notarization complete")


def main() -> None:
    if sys.platform != "darwin":
        sys.exit("Error: this script only runs on macOS")

    parser = argparse.ArgumentParser(description="Package ruf4 as a macOS .app bundle")
    parser.add_argument("--sign", action="store_true", help="Code-sign the app bundle")
    parser.add_argument("--notarize", action="store_true", help="Notarize the app bundle")
    args = parser.parse_args()

    root = Path(__file__).resolve().parent.parent
    binary = Path(os.environ.get("BINARY_PATH", root / "target" / "release" / "ruf4"))
    version = read_version(root)

    if not binary.exists():
        sys.exit(f"Error: binary not found at {binary}\nRun 'cargo build --release' first.")

    print(f"==> Packaging ruf4 v{version} as ruf4.app")
    app_dir = build_bundle(root, binary, version)
    print(f"==> App bundle created at {app_dir}")

    if args.sign:
        sign(root, app_dir)

    if args.notarize:
        notarize(root, app_dir)

    print(f"==> Done: {app_dir}")


if __name__ == "__main__":
    main()
