#!/usr/bin/env python3
"""Fail CI if a public fn or a mounted route has no production caller.

A unit test that invokes the function directly is exactly what production
does not do — five defects in this repo were that shape. Exceptions live
in scripts/wired-exceptions.txt in two sections: Rinvii (future phase)
and Debiti (shipped in a closed phase; the third field is who pays).
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


IMPORT_RE = re.compile(
    r"""(?:from\s+|import\s*\(\s*)['"]([^'"]+)['"]""",
)


def _is_spec(path: Path) -> bool:
    return path.name.endswith(".spec.ts") or path.name.endswith(".spec.vue")


def resolve_frontend_import(src: Path, importer: Path, spec: str) -> Path | None:
    """Resolve a TS/Vue import to a file under `frontend/src`, or None."""
    if spec.startswith("@/"):
        raw = src / spec[2:]
    elif spec.startswith("."):
        raw = importer.parent / spec
    else:
        return None
    candidates: list[Path] = []
    if raw.suffix in {".ts", ".vue", ".js"}:
        candidates.append(raw)
    else:
        candidates.extend(
            [
                Path(str(raw) + ".ts"),
                Path(str(raw) + ".vue"),
                Path(str(raw) + ".js"),
                raw / "index.ts",
                raw / "index.js",
            ]
        )
    src_root = src.resolve()
    for cand in candidates:
        try:
            resolved = cand.resolve()
            resolved.relative_to(src_root)
        except (OSError, ValueError):
            continue
        if resolved.is_file() and not _is_spec(resolved):
            return resolved
    return None


def vue_reachable_frontend_files(src: Path) -> list[Path]:
    """Files a non-spec `.vue` can reach by following imports.

    A route string that lives only in `api/*.ts` — never imported from a
    view or component — is not a production consumer. V5/V6 used to pass
    that way: `permissions.ts` mentioned `/api/v1/permissions` and no
    `.vue` imported it.
    """
    reachable: set[Path] = set()
    queue: list[Path] = [
        p for p in src.rglob("*.vue") if not _is_spec(p)
    ]
    while queue:
        path = queue.pop()
        resolved = path.resolve()
        if resolved in reachable:
            continue
        reachable.add(resolved)
        try:
            text = path.read_text()
        except OSError:
            continue
        for spec in IMPORT_RE.findall(text):
            imported = resolve_frontend_import(src, path, spec)
            if imported is not None and imported not in reachable:
                queue.append(imported)
    return sorted(reachable)


def frontend_mentions(needle: str) -> bool:
    src = ROOT / "frontend" / "src"
    for path in vue_reachable_frontend_files(src):
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
