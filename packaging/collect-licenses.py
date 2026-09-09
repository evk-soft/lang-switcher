#!/usr/bin/env python3
"""Builds THIRD-PARTY-LICENSES.md for the shipped Windows binary.

Why a script and not `cargo-about`: the release must be reproducible from a pinned
toolchain plus this repository, and adding a cargo plugin to the release job means one
more unpinned tool that has to be installed and trusted. Everything needed is already
here — `cargo tree` knows the dependency set and the registry holds each crate's own
licence files.

Only `--edges normal` dependencies for the Windows target are included. Build-time and
development dependencies are not part of the distributed artifact, so their licences do
not have to travel with it.

Usage:
    python packaging/collect-licenses.py --output target/packaging/THIRD-PARTY-LICENSES.md

Exits non-zero if any crate could not be resolved; a release must not ship an incomplete
notice.
"""

from __future__ import annotations

import argparse
import glob
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

TARGET = "x86_64-pc-windows-msvc"
PACKAGE = "switcher-app"

# Files a crate uses to ship its licence text.
LICENSE_PATTERNS = (
    "LICENSE*",
    "LICENCE*",
    "COPYING*",
    "UNLICENSE*",
    "NOTICE*",
)

# The workspace's own crates. Our licence is stated by the release itself, not as a
# third-party notice.
LOCAL_CRATES = {
    "switcher-app",
    "switcher-core",
    "switcher-platform",
    "switcher-windows",
}


def dependency_set(root: Path) -> list[tuple[str, str]]:
    """(name, version) of every crate linked into the Windows binary."""
    out = subprocess.run(
        [
            "cargo", "tree",
            "-p", PACKAGE,
            "--edges", "normal",
            "--target", TARGET,
            "--prefix", "none",
            "--format", "{p}",
        ],
        cwd=root,
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=True,
    ).stdout

    found: set[tuple[str, str]] = set()
    for line in out.splitlines():
        # "name v1.2.3", optionally followed by " (proc-macro)", " (*)" or a local path.
        match = re.match(r"^(\S+)\s+v(\S+)", line.strip())
        if not match:
            continue
        name, version = match.group(1), match.group(2)
        if name not in LOCAL_CRATES:
            found.add((name, version))
    return sorted(found)


def source_dir(root: Path, name: str, version: str) -> Path | None:
    """Where the crate's extracted source lives: the registry, or vendor/ for a patch."""
    home = os.environ.get("CARGO_HOME") or str(Path.home() / ".cargo")
    for candidate in glob.glob(f"{home}/registry/src/*/{name}-{version}"):
        return Path(candidate)
    vendored = root / "vendor" / name
    return vendored if vendored.is_dir() else None


def license_texts(crate: Path) -> list[tuple[str, str]]:
    seen: dict[str, str] = {}
    for pattern in LICENSE_PATTERNS:
        for path in sorted(crate.glob(pattern)):
            if not path.is_file() or path.name in seen:
                continue
            try:
                seen[path.name] = path.read_text(encoding="utf-8", errors="replace")
            except OSError as error:
                print(f"warning: cannot read {path}: {error}", file=sys.stderr)
    return sorted(seen.items())


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parent.parent,
        help="repository root (defaults to the parent of packaging/)",
    )
    args = parser.parse_args()
    root: Path = args.root

    crates = dependency_set(root)
    if not crates:
        print("error: cargo tree returned no dependencies", file=sys.stderr)
        return 1

    # Nearly every crate ships the same two files, the Apache-2.0 and MIT texts. Emitting
    # them per crate produced about a megabyte of near-identical text, so each distinct
    # text is written once in an appendix and referenced from the table.
    texts: dict[str, int] = {}
    rows: list[str] = []
    missing: list[str] = []
    expressions: set[str] = set()

    for name, version in crates:
        directory = source_dir(root, name, version)
        if directory is None:
            missing.append(f"{name} {version}: source directory not found")
            continue
        try:
            manifest = tomllib.loads((directory / "Cargo.toml").read_text(encoding="utf-8"))
        except (OSError, tomllib.TOMLDecodeError) as error:
            missing.append(f"{name} {version}: unreadable Cargo.toml ({error})")
            continue

        package = manifest.get("package", {})
        expression = package.get("license")
        if expression:
            expressions.add(expression)
        repository = package.get("repository")

        references = []
        for filename, body in license_texts(directory):
            normalized = body.replace(os.linesep, "\n").replace("\r\n", "\n").rstrip()
            index = texts.setdefault(normalized, len(texts) + 1)
            references.append(f"[{filename}](#licence-text-{index})")

        rows.append(
            "| "
            + " | ".join(
                [
                    f"`{name}`",
                    version,
                    f"`{expression}`" if expression else "see text",
                    f"<{repository}>" if repository else "—",
                    # A crate with no file ships only the SPDX grant in its manifest.
                    ", ".join(references) if references else "SPDX grant only",
                ]
            )
            + " |"
        )

    lines = [
        "# Third-party licences",
        "",
        "lang-switcher itself is dual-licensed under MIT or Apache-2.0; see `LICENSE-MIT`",
        "and `LICENSE-APACHE`. The embedded Inter SemiBold subset is covered by SIL OFL 1.1.",
        "",
        f"Every crate compiled into `lang-switcher.exe` for `{TARGET}` is listed below",
        f"({len(crates)} crates). Build-time and test-only dependencies are not distributed",
        "and are therefore not included.",
        "",
        "Generated by `packaging/collect-licenses.py`; do not edit by hand.",
        "",
        "## Licence expressions in use",
        "",
    ]
    lines += [f"- `{expression}`" for expression in sorted(expressions)]
    lines.append("")

    if missing:
        lines += [
            "## Unresolved",
            "",
            "The generator could not read these crates. A release must not ship while",
            "anything is listed here.",
            "",
        ]
        lines += [f"- {item}" for item in missing]
        lines.append("")

    lines += [
        "## Crates",
        "",
        "| Crate | Version | Licence | Source | Licence text |",
        "| --- | --- | --- | --- | --- |",
        *rows,
        "",
        "## Licence texts",
        "",
    ]
    for body, index in sorted(texts.items(), key=lambda item: item[1]):
        lines += [
            f'### Licence text {index} <a id="licence-text-{index}"></a>',
            "",
            "```text",
            body,
            "```",
            "",
        ]

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    print(
        f"wrote {args.output}: {len(crates)} crates, "
        f"{len(texts)} distinct licence texts, {len(missing)} unresolved"
    )
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main())
