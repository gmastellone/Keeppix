#!/usr/bin/env python3
"""Task 18 — cold-start geometry transfer budget (fase-10).

Measures the shipped binary format (8-byte header + 6 bytes/record, no uuid)
and estimates first layout-ready paint on throttled mobile profiles.

Run from repo root: python3 scripts/measure-geometry-mobile.py
"""

from __future__ import annotations

import gzip
import io
import struct
import time

N_SPEC = 214_000  # plan reference library size
N_SCALE = 200_000  # keeppix-db scale_geometry.rs

# Chrome DevTools-ish profiles: (name, downlink bps, RTT seconds, RTT rounds)
PROFILES = (
    ("Fast 3G", 1.6e6, 0.150, 3),
    ("Slow 3G", 400e3, 0.400, 3),
    ("4G", 4e6, 0.020, 2),
)

# keeppix-db/tests/scale_geometry.rs (2025-08-21): repo.geometry @ 200k
SERVER_MS = 591
# Client: gzip decompress + single DataView scan (fase-11 Task 4)
CLIENT_MS = 32


def encode_records(count: int, gen) -> bytes:
    buf = io.BytesIO()
    buf.write(struct.pack("<II", 1, count))
    for i in range(count):
        w, h, m = gen(i)
        buf.write(struct.pack("<HHH", w & 0xFFFF, h & 0xFFFF, m & 0xFFFF))
    return buf.getvalue()


def gzip_level6(raw: bytes) -> bytes:
    return gzip.compress(raw, compresslevel=6)


def client_ms(raw_gz: bytes, count: int) -> float:
    t0 = time.perf_counter()
    raw = gzip.decompress(raw_gz)
    view = memoryview(raw)
    offset = 8
    for _ in range(count):
        struct.unpack_from("<HHH", view, offset)
        offset += 6
    return (time.perf_counter() - t0) * 1000


def cold_start_s(gz_len: int, bps: float, rtt: float, rounds: int, client: float) -> float:
    return rounds * rtt + (gz_len * 8) / bps + SERVER_MS / 1000 + client / 1000


def report(label: str, raw: bytes, gz: bytes, count: int) -> None:
    measured_client = client_ms(gz, count)
    print(f"\n=== {label} (N={count:,}) ===")
    print(f"raw_bytes={len(raw):,} ({len(raw) / 1024 / 1024:.3f} MiB)")
    print(
        f"gzip_bytes={len(gz):,} ({len(gz) / 1024:.1f} KiB, "
        f"{len(gz) / 1024 / 1024:.3f} MiB) ratio={len(raw) / len(gz):.2f}x"
    )
    print(f"client_decode_measured_ms={measured_client:.1f}")
    print("cold_start_first_paint_estimate (RTT rounds + transfer + server + client):")
    for name, bps, rtt, rounds in PROFILES:
        total = cold_start_s(len(gz), bps, rtt, rounds, measured_client)
        flag = "EXCEEDS 2s" if total > 2.0 else "under 2s"
        print(f"  {name:10s} {total:.3f}s ({flag})")


def main() -> None:
    # Spec §2.3 reference: realistic EXIF spread (several bodies, month buckets).
    spec_like = encode_records(
        N_SPEC,
        lambda i: (
            [6000, 4000, 4032, 3024, 5184][i % 5],
            [4000, 3000, 3024, 3024, 3456][i % 5],
            (2000 + (i // 8000) % 20) * 12 + (1 + (i // 650) % 12),
        ),
    )
    report("spec-like library", spec_like, gzip_level6(spec_like), N_SPEC)

    # Upper bound: independent w/h/month — gzip worst case for this format.
    max_entropy = encode_records(
        N_SPEC,
        lambda i: (
            2000 + (i * 17) % 4000,
            1500 + (i * 23) % 3000,
            (1980 + (i * 7919) % 45) * 12 + (1 + (i * 13) % 12),
        ),
    )
    report("max-entropy synthetic", max_entropy, gzip_level6(max_entropy), N_SPEC)

    # Spec §2.3 published measurement on 214k realistic records (not re-derived here).
    spec_gzip = 451 * 1024
    print(f"\n=== spec §2.3 reference (451 KiB gzip, 214k realistic records) ===")
    print(f"gzip_bytes={spec_gzip:,} ({spec_gzip / 1024:.0f} KiB) — from fase-10-api-interfaccia.md §2.3")
    print("cold_start_first_paint_estimate:")
    for name, bps, rtt, rounds in PROFILES:
        total = cold_start_s(spec_gzip, bps, rtt, rounds, CLIENT_MS)
        flag = "EXCEEDS 2s" if total > 2.0 else "under 2s"
        print(f"  {name:10s} {total:.3f}s ({flag})")

    raw_anchor = 8 + N_SPEC * 6
    print(f"\n=== format anchor ===")
    print(f"raw_bytes@{N_SPEC:,} = {raw_anchor:,} (8 + N*6, always)")


if __name__ == "__main__":
    main()
