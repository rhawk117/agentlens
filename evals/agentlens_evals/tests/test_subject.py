from pathlib import Path

import pytest

from agentlens_evals import subject


@pytest.fixture()
def bin_root(tmp_path: Path, monkeypatch) -> Path:
    root = tmp_path / "bin"
    monkeypatch.setattr(subject, "BIN_ROOT", root)
    return root


def fake_install(bin_root: Path, version: str = "9.9.9") -> Path:
    binary = subject.binary_path(version)
    binary.parent.mkdir(parents=True)
    binary.write_bytes(b"#!/bin/sh\necho agentlens 9.9.9\n")
    subject.write_manifest(version, binary, "deadbeef", "2026-08-17T00:00:00+00:00")
    return binary


def test_verify_accepts_matching_hash(bin_root) -> None:
    binary = fake_install(bin_root)
    assert subject.verify("9.9.9") == binary


def test_verify_refuses_tampered_binary(bin_root) -> None:
    binary = fake_install(bin_root)
    binary.write_bytes(b"#!/bin/sh\necho tampered\n")
    with pytest.raises(subject.SubjectError, match="sha256 mismatch"):
        subject.verify("9.9.9")


def test_verify_refuses_missing_manifest(bin_root) -> None:
    binary = subject.binary_path("9.9.9")
    binary.parent.mkdir(parents=True)
    binary.write_bytes(b"x")
    with pytest.raises(subject.SubjectError, match="manifest"):
        subject.verify("9.9.9")


def test_verify_refuses_version_mismatch(bin_root) -> None:
    fake_install(bin_root, "9.9.9")
    with pytest.raises(subject.SubjectError):
        subject.verify("8.8.8")
