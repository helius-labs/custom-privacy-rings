"""Paths, names and digests the ring key tools share."""

import hashlib
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / "prover/ring/proving-keys.lock"


def catalog():
    return json.loads(subprocess.check_output(["go", "run", "./cmd/catalog"], cwd=ROOT / "prover"))


def key_file(key):
    return key["Circuit"].replace("-", "_") + ".key"


def verifier_module(name):
    return name.removeprefix("custom_ring_").removesuffix(".key") + "_verifying_key"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def verifier_relative_path(module: str) -> Path:
    name = module.removesuffix(".rs")
    file = f"{name}.rs"
    if name.startswith("transfer_"):
        return Path("transfer") / file
    if name.startswith("merge_"):
        return Path("merge") / file
    if name.startswith(("apply_records_", "apply_enrollments_", "register_key_")):
        return Path("records") / file
    raise ValueError(f"unknown ring verifier family: {module}")
