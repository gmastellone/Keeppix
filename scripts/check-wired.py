#!/usr/bin/env python3
"""Fail CI if a public fn or a mounted route has no production caller.

A unit test that invokes the function directly is exactly what production
does not do — five defects in this repo were that shape. Exceptions live
in scripts/wired-exceptions.txt with the phase that will consume them.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXCEPTIONS = ROOT / "scripts" / "wired-exceptions.txt"

FN_RE = re.compile(
    r"^\s*pub(?:\s*\([^)]*\))?\s+(?:async\s+)?fn\s+([A-Za-z0-9_]+)\s*[<(]",
    re.M,
)
ROUTE_RE = re.compile(r'\.route\(\s*"([^"]+)"')
CFG_TEST_RE = re.compile(
    r"#\[cfg\(test\)\][\s\S]*?(?=\n(?:pub |fn |struct |enum |impl |mod |#\[))",
)


def load_exceptions() -> tuple[set[str], set[str]]:
    fns: set[str] = set()
    routes: set[str] = set()
    if not EXCEPTIONS.exists():
        return fns, routes
    for raw in EXCEPTIONS.read_text().splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        kind, _, rest = line.partition(" ")
        name, _, phase = rest.partition(" ")
        if not phase.strip():
            sys.exit(f"{EXCEPTIONS}: exception without phase: {raw}")
        if kind == "fn":
            fns.add(name.strip())
        elif kind == "route":
            routes.add(name.strip())
        else:
            sys.exit(f"{EXCEPTIONS}: unknown kind {kind!r}")
    return fns, routes


def rust_prod_files(crate: str) -> list[Path]:
    src = ROOT / "crates" / crate / "src"
    return [
        p
        for p in src.rglob("*.rs")
        if "tests" not in p.parts and p.name != "tests.rs"
    ]


def all_prod_rust() -> list[Path]:
    files: list[Path] = []
    for crate_dir in (ROOT / "crates").iterdir():
        src = crate_dir / "src"
        if src.is_dir():
            files.extend(
                p
                for p in src.rglob("*.rs")
                if "tests" not in p.parts and p.name != "tests.rs"
            )
    return files


def strip_cfg_test(text: str) -> str:
    return CFG_TEST_RE.sub("", text)


def defined_public_fns(crate: str) -> dict[str, Path]:
    found: dict[str, Path] = {}
    for path in rust_prod_files(crate):
        text = strip_cfg_test(path.read_text())
        for match in FN_RE.finditer(text):
            name = match.group(1)
            if name in {"new", "default", "from", "fmt", "clone", "eq"}:
                continue
            found.setdefault(name, path)
    return found


def count_ident(name: str, files: list[Path], definition: Path) -> int:
    pat = re.compile(rf"\b{re.escape(name)}\b")
    n = 0
    for path in files:
        text = strip_cfg_test(path.read_text())
        hits = len(pat.findall(text))
        if path == definition:
            hits -= 1  # the definition itself
            if hits < 0:
                hits = 0
        n += hits
    return n


def mounted_routes() -> list[str]:
    lib = ROOT / "crates" / "keeppix-api" / "src" / "lib.rs"
    seen: set[str] = set()
    out: list[str] = []
    for route in ROUTE_RE.findall(lib.read_text()):
        if route not in seen:
            seen.add(route)
            out.append(route)
    return out


def route_needle(route: str) -> str:
    # `/media/thumb/{hash}` → `/media/thumb`
    # `/ws/ticket` → `/ws/ticket`
    return re.sub(r"/\{[^}]+\}.*$", "", route) or route


def frontend_mentions(needle: str) -> bool:
    src = ROOT / "frontend" / "src"
    paths = [
        p
        for p in list(src.rglob("*.ts")) + list(src.rglob("*.vue"))
        if not p.name.endswith(".spec.ts") and not p.name.endswith(".spec.vue")
    ]
    for path in paths:
        if needle in path.read_text():
            return True
    return False


def main() -> int:
    except_fns, except_routes = load_exceptions()
    prod = all_prod_rust()
    unused_fns: list[str] = []
    for crate in ("keeppix-db", "keeppix-media"):
        for name, path in sorted(defined_public_fns(crate).items()):
            key = f"{crate}::{name}"
            if name in except_fns or key in except_fns:
                continue
            if count_ident(name, prod, path) == 0:
                unused_fns.append(f"{key} ({path.relative_to(ROOT)})")

    unused_routes: list[str] = []
    for route in mounted_routes():
        if route in except_routes:
            continue
        needle = route_needle(route)
        if not frontend_mentions(needle):
            unused_routes.append(route)

    if unused_fns or unused_routes:
        if unused_fns:
            print("public functions with no production caller:")
            for item in unused_fns:
                print(f"  {item}")
        if unused_routes:
            print("mounted routes with no frontend consumer:")
            for item in unused_routes:
                print(f"  {item}")
        print(
            "\nIf this is waiting for a later phase, add it to "
            "scripts/wired-exceptions.txt with that phase."
        )
        return 1
    print("all public fns and mounted routes have a production caller")
    return 0


if __name__ == "__main__":
    sys.exit(main())
