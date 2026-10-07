#!/usr/bin/env python3
"""Reproduce the pre-refactor/current policy CPU matrix without GPUI or models."""
import argparse
import hashlib
import json
import platform
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
BASE = "68960389525c8f7ae4eb7fc62eb7a242dd9b648c"


def run(*args, cwd=ROOT):
    return subprocess.check_output(args, cwd=cwd, text=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / ".qualification/restoration/reproduced")
    output = parser.parse_args().output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    baseline = output / "baseline"
    baseline.mkdir(exist_ok=True)
    old_source = run("git", "show", f"{BASE}:src/pseudonymization.rs")
    (baseline / "baseline_policy.rs").write_text(old_source)
    shutil.copy2(ROOT / "tools/pseudonymization/policy_baseline.rs", baseline / "main.rs")
    subprocess.run(["rustc", "-Awarnings", "-O", "--edition=2024", "main.rs", "-o", "bench"], cwd=baseline, check=True)
    (output / "baseline.txt").write_text(run(str(baseline / "bench")))
    current = output / "current"
    source = current / "src"
    source.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / "tools/pseudonymization/policy_bench.rs", source / "main.rs")
    shutil.copy2(ROOT / "src/pseudonymization.rs", source / "policy.rs")
    shutil.copytree(ROOT / "src/pseudonymization", source / "policy", dirs_exist_ok=True)
    shutil.copy2(ROOT / "crates/mdoc-editor/src/transactions.rs", source / "transactions.rs")
    (current / "Cargo.toml").write_text('[package]\nname="restoration-policy-benchmark"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\naho-corasick="=1.1.5"\n')
    subprocess.run(["cargo", "generate-lockfile", "--offline"], cwd=current, check=True)
    (output / "current.txt").write_text(run("cargo", "run", "--release", "--offline", "--locked", "-q", cwd=current))
    files = [ROOT / "src/pseudonymization.rs", ROOT / "crates/mdoc-editor/src/transactions.rs", *sorted((ROOT / "src/pseudonymization").glob("*.rs")), ROOT / "tools/pseudonymization/policy_bench.rs", ROOT / "tools/pseudonymization/policy_baseline.rs"]
    provenance = {"baseline_commit": BASE, "baseline_policy_sha256": hashlib.sha256(old_source.encode()).hexdigest(), "rustc": run("rustc", "--version").strip(), "platform": platform.platform(), "source_sha256": {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in files}}
    (output / "provenance.json").write_text(json.dumps(provenance, indent=2)+"\n")
    print((output / "baseline.txt").read_text(), end="")
    print((output / "current.txt").read_text(), end="")


if __name__ == "__main__":
    main()
