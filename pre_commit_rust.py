#!/usr/bin/env python3
import argparse
import glob
import subprocess
from functools import lru_cache
from pathlib import Path

ACTIONS = ("fmt", "check", "clippy")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.set_defaults(func=None)
    sps = parser.add_subparsers(dest="cmd")

    sp = sps.add_parser("fmt")
    sp.add_argument(
        "--config",
        type=str,
        help="Comma-separated key=value config pairs for rustfmt",
    )
    sp.set_defaults(func=run_fmt)
    add_files_nargs(sp)

    sp = sps.add_parser("check")
    sp.add_argument(
        "--features",
        type=str,
        help="Space or comma-separated list of features to check",
    )
    sp.add_argument(
        "--all-features",
        action="store_true",
        help="Activate all available features",
    )
    sp.set_defaults(func=run_check)
    add_files_nargs(sp)

    sp = sps.add_parser("clippy")
    sp.set_defaults(func=run_clippy)
    add_files_nargs(sp)

    args = parser.parse_args()

    if args.func is None:
        parser.print_help()
        return 1

    run_dirs = get_run_dirs(args.files)
    if not run_dirs:
        return 0

    failed = sum(args.func(args, d) for d in run_dirs)
    return int(failed > 0)


def add_files_nargs(parser: argparse.ArgumentParser):
    parser.add_argument(
        "files",
        nargs="*",
        type=Path,
    )


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


def run_fmt(args: argparse.Namespace, directory: Path) -> int:
    cmd = "cargo fmt --"
    if args.config:
        cmd += f" --config {args.config}"
    return run_action(cmd, directory)


def run_check(args: argparse.Namespace, directory: Path) -> int:
    cmd = "cargo check"
    if args.features is not None:
        cmd += f" --features={args.features}"
    if args.all_features:
        cmd += f" --all-features"
    return run_action(cmd, directory)


def run_clippy(_: argparse.Namespace, directory: Path) -> int:
    cmd = "cargo clippy -- -D warnings"
    return run_action(cmd, directory)


def run_action(cmd: str, directory: Path) -> int:
    proc = subprocess.run(cmd, cwd=directory, shell=True)
    return proc.returncode


if __name__ == "__main__":
    exit(main())
