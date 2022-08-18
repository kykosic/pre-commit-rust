#!/usr/bin/env python3
import argparse
import glob
import subprocess
from functools import lru_cache
from pathlib import Path

ACTIONS = ("fmt", "check", "clippy")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "action",
        choices=ACTIONS,
    )
    parser.add_argument(
        "files",
        nargs="*",
        type=Path,
    )
    args = parser.parse_args()

    run_dirs = get_run_dirs(args.files)
    if not run_dirs:
        return 0

    failed = sum(run_action(args.action, d) for d in run_dirs)
    return int(failed > 0)


def get_run_dirs(changed_files: list[Path]) -> set[Path]:
    root_dirs = find_cargo_root_dirs()
    run_dirs: set[Path] = set()
    for path in changed_files:
        if not is_rust_file(path):
            continue
        roots = [d for d in root_dirs if path.is_relative_to(d)]
        if not roots:
            continue
        root = max(roots, key=path_len)
        run_dirs.add(root)
    return run_dirs


def find_cargo_root_dirs() -> list[Path]:
    return [Path(p).parent for p in glob.glob("**/Cargo.toml", recursive=True)]


def is_rust_file(path: Path) -> bool:
    if path.suffix == ".rs":
        return True
    elif path.name in ["Cargo.toml", "Cargo.lock"]:
        return True
    return False


@lru_cache
def path_len(path: Path) -> int:
    return len(path.parts)


def run_action(action: str, directory: Path) -> int:
    if action == "fmt":
        cmd = "cargo fmt --"
    elif action == "check":
        cmd = "cargo check"
    elif action == "clippy":
        cmd = "cargo clippy -- -D warnings"
    else:
        raise ValueError(f"Invalid action {action!r}, expected one of: {ACTIONS}")

    proc = subprocess.run(f"{cmd}", cwd=directory, shell=True)
    return proc.returncode


if __name__ == "__main__":
    exit(main())
