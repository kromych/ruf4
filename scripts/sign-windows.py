#!/usr/bin/env python3
"""Sign a Windows binary using Azure Trusted Signing.

Usage:
    python3 scripts/sign-windows.py [binary ...]

If no binaries are given, defaults to target/release/ruf4.exe.

Environment variables (all required):
    AZURE_TENANT_ID       - Azure AD tenant ID
    AZURE_CLIENT_ID       - App registration client ID
    AZURE_CLIENT_SECRET   - App registration client secret
    AZURE_ENDPOINT        - Trusted Signing account endpoint
                            (e.g. https://wus2.codesigning.azure.net)
    AZURE_ACCOUNT_NAME    - Trusted Signing account name
    AZURE_CERT_PROFILE    - Certificate profile name
"""

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path


REQUIRED_ENV = [
    "AZURE_TENANT_ID",
    "AZURE_CLIENT_ID",
    "AZURE_CLIENT_SECRET",
    "AZURE_ENDPOINT",
    "AZURE_ACCOUNT_NAME",
    "AZURE_CERT_PROFILE",
]


def run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    print(f"    $ {' '.join(cmd)}")
    return subprocess.run(cmd, check=True, **kwargs)


def check_env() -> dict[str, str]:
    env = {}
    missing = []
    for var in REQUIRED_ENV:
        val = os.environ.get(var)
        if not val:
            missing.append(var)
        else:
            env[var] = val
    if missing:
        sys.exit(f"Error: missing environment variables: {', '.join(missing)}")
    return env


def find_signtool() -> str:
    signtool = shutil.which("signtool")
    if signtool:
        return signtool

    # Common Windows SDK locations.
    sdk_root = Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)"))
    sdk_bin = sdk_root / "Windows Kits" / "10" / "bin"
    if sdk_bin.is_dir():
        candidates = sorted(sdk_bin.glob("*/x64/signtool.exe"), reverse=True)
        if candidates:
            return str(candidates[0])

    sys.exit("Error: signtool.exe not found. Install the Windows SDK.")


def find_dlib() -> str:
    dlib = shutil.which("Azure.CodeSigning.Dlib.dll")
    if dlib:
        return dlib

    # Installed via dotnet tool or NuGet.
    home = Path.home()
    candidates = list(home.glob(".nuget/packages/microsoft.trusted.signing.client/*/bin/x64/Azure.CodeSigning.Dlib.dll"))
    if not candidates:
        candidates = list(Path(r"C:\Users").glob("*/.nuget/packages/microsoft.trusted.signing.client/*/bin/x64/Azure.CodeSigning.Dlib.dll"))
    if candidates:
        return str(sorted(candidates, reverse=True)[0])

    sys.exit(
        "Error: Azure.CodeSigning.Dlib.dll not found.\n"
        "Install it with: dotnet tool install --global Microsoft.Trusted.Signing.Client"
    )


def write_metadata(env: dict[str, str], path: Path) -> Path:
    metadata = path.parent / "signing-metadata.json"
    import json
    meta = {
        "Endpoint": env["AZURE_ENDPOINT"],
        "CodeSigningAccountName": env["AZURE_ACCOUNT_NAME"],
        "CertificateProfileName": env["AZURE_CERT_PROFILE"],
    }
    metadata.write_text(json.dumps(meta, indent=2))
    return metadata


def sign(binary: Path, env: dict[str, str]) -> None:
    if not binary.exists():
        sys.exit(f"Error: binary not found at {binary}")

    signtool = find_signtool()
    dlib = find_dlib()
    metadata = write_metadata(env, binary)

    print(f"==> Signing {binary}")
    try:
        run([
            signtool, "sign",
            "/v",
            "/fd", "SHA256",
            "/tr", "http://timestamp.acs.microsoft.com",
            "/td", "SHA256",
            "/dlib", dlib,
            "/dmdf", str(metadata),
            str(binary),
        ])
    finally:
        metadata.unlink(missing_ok=True)

    print(f"==> Verifying signature on {binary}")
    run([signtool, "verify", "/v", "/pa", str(binary)])
    print("==> Signature OK")


def main() -> None:
    if sys.platform != "win32":
        sys.exit("Error: this script only runs on Windows")

    parser = argparse.ArgumentParser(description="Sign Windows binaries with Azure Trusted Signing")
    parser.add_argument("binaries", nargs="*", help="Paths to binaries to sign")
    args = parser.parse_args()

    env = check_env()

    root = Path(__file__).resolve().parent.parent
    binaries = [Path(b) for b in args.binaries] if args.binaries else [root / "target" / "release" / "ruf4.exe"]

    for binary in binaries:
        sign(binary, env)

    print(f"==> Done: signed {len(binaries)} file(s)")


if __name__ == "__main__":
    main()
