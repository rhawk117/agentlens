"""The binary under test, pinned by version and hash.

The v0.2.0 campaign was contaminated because the wrapper resolved
target/release/agentlens, a path any `cargo build` mutates. Here a campaign
binary lives in gitignored evals/bin/<version>/ beside a manifest recording
what it is, and every campaign command re-hashes it and refuses on mismatch.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
from datetime import datetime, timezone
from pathlib import Path

from agentlens_evals.paths import BIN_ROOT, REPO_ROOT


class SubjectError(RuntimeError):
    """The binary under test cannot be trusted; refuse to run."""


def binary_path(version: str) -> Path:
    return BIN_ROOT / version / "agentlens"


def manifest_path(version: str) -> Path:
    return BIN_ROOT / version / "manifest.json"


def sha256_of(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_manifest(
    version: str, binary: Path, source_commit: str, built_at: str
) -> None:
    manifest_path(version).write_text(
        json.dumps(
            {
                "version": version,
                "sha256": sha256_of(binary),
                "source_commit": source_commit,
                "built_at": built_at,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )


def install(version: str) -> Path:
    subprocess.run(["cargo", "build", "--release"], cwd=REPO_ROOT, check=True)
    built = REPO_ROOT / "target" / "release" / "agentlens"
    reported = subprocess.run(
        [str(built), "--version"], capture_output=True, text=True, check=True
    ).stdout.strip()
    if version not in reported:
        raise SubjectError(
            f"built binary reports {reported!r}, expected version {version}"
        )
    commit = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    destination = binary_path(version)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(built, destination)
    write_manifest(version, destination, commit, datetime.now(timezone.utc).isoformat())
    return destination


def verify(version: str) -> Path:
    binary = binary_path(version)
    manifest_file = manifest_path(version)
    if not binary.is_file():
        raise SubjectError(
            f"no installed binary for {version}: run `agentlens-evals install`"
        )
    if not manifest_file.is_file():
        raise SubjectError(f"missing manifest for {version}")
    manifest = json.loads(manifest_file.read_text(encoding="utf-8"))
    if manifest.get("version") != version:
        raise SubjectError(
            f"manifest records version {manifest.get('version')!r}, expected {version!r}"
        )
    actual = sha256_of(binary)
    if actual != manifest.get("sha256"):
        raise SubjectError(
            f"sha256 mismatch for {binary}: manifest {manifest.get('sha256')}, actual {actual}"
        )
    return binary
