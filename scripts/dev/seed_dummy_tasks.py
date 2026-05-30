#!/usr/bin/env python3

import argparse
import os
import pathlib
import subprocess
import sys


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--env", default="development")
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--start", type=int, default=1)
    parser.add_argument("--count", type=int, default=24)
    parser.add_argument("--project")
    args = parser.parse_args()

    repo_root = pathlib.Path(__file__).resolve().parents[2]
    helper_manifest = repo_root / "scripts" / "dev" / "seed_helper" / "Cargo.toml"

    subprocess.run(
        ["cargo", "build", "--release", "--manifest-path", str(helper_manifest)],
        cwd=repo_root,
        check=True,
    )

    helper_bin = repo_root / "scripts" / "dev" / "seed_helper" / "target" / "release" / "taskforce-seed-helper"
    command = [
        str(helper_bin),
        "--env",
        args.env,
        "--seed",
        str(args.seed),
        "--start",
        str(args.start),
        "--count",
        str(args.count),
    ]
    if args.project:
        command.extend(["--project", args.project])

    result = subprocess.run(command, cwd=repo_root)
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
