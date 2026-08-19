#!/usr/bin/env python3
"""Normalize and simplify timezone-boundary-builder GeoJSON into Keeppix TSV."""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path
from typing import Any


def point_segment_distance_sq(
    point: list[float], start: list[float], end: list[float]
) -> float:
    dx = end[0] - start[0]
    dy = end[1] - start[1]
    if dx == 0.0 and dy == 0.0:
        return (point[0] - start[0]) ** 2 + (point[1] - start[1]) ** 2
    ratio = max(
        0.0,
        min(
            1.0,
            ((point[0] - start[0]) * dx + (point[1] - start[1]) * dy)
            / (dx * dx + dy * dy),
        ),
    )
    projected = [start[0] + ratio * dx, start[1] + ratio * dy]
    return (point[0] - projected[0]) ** 2 + (point[1] - projected[1]) ** 2


def simplify_line(points: list[list[float]], epsilon: float) -> list[list[float]]:
    if len(points) <= 2:
        return points
    keep = {0, len(points) - 1}
    stack = [(0, len(points) - 1)]
    threshold = epsilon * epsilon
    while stack:
        start_index, end_index = stack.pop()
        farthest_index = -1
        farthest_distance = threshold
        for index in range(start_index + 1, end_index):
            distance = point_segment_distance_sq(
                points[index], points[start_index], points[end_index]
            )
            if distance > farthest_distance:
                farthest_distance = distance
                farthest_index = index
        if farthest_index >= 0:
            keep.add(farthest_index)
            stack.append((start_index, farthest_index))
            stack.append((farthest_index, end_index))
    return [point for index, point in enumerate(points) if index in keep]


def normalize_point(raw: Any) -> list[float]:
    if not isinstance(raw, list) or len(raw) < 2:
        raise ValueError("coordinate is not a longitude/latitude pair")
    lon = float(raw[0])
    lat = float(raw[1])
    if not math.isfinite(lon) or not math.isfinite(lat):
        raise ValueError("coordinate is not finite")
    return [round(lon, 5), round(lat, 5)]


def simplify_ring(raw_ring: Any, epsilon: float) -> list[list[float]]:
    if not isinstance(raw_ring, list):
        raise ValueError("polygon ring is not an array")
    points = [normalize_point(point) for point in raw_ring]
    if len(points) < 4:
        raise ValueError("polygon ring has fewer than four points")
    if points[0] == points[-1]:
        points.pop()
    if len(points) < 3:
        raise ValueError("polygon ring has fewer than three distinct points")

    start_index = min(range(len(points)), key=lambda index: tuple(points[index]))
    points = points[start_index:] + points[:start_index]
    split_index = max(
        range(1, len(points)),
        key=lambda index: (
            points[index][0] - points[0][0]
        ) ** 2
        + (points[index][1] - points[0][1]) ** 2,
    )
    first = simplify_line(points[: split_index + 1], epsilon)
    second = simplify_line(points[split_index:] + [points[0]], epsilon)
    simplified = first[:-1] + second
    if len(simplified) < 4:
        simplified = points + [points[0]]
    return simplified


def normalize_geometry(raw: Any, epsilon: float) -> list[list[list[list[float]]]]:
    if not isinstance(raw, dict):
        raise ValueError("feature geometry is missing")
    geometry_type = raw.get("type")
    coordinates = raw.get("coordinates")
    if geometry_type == "Polygon":
        polygons = [coordinates]
    elif geometry_type == "MultiPolygon":
        polygons = coordinates
    else:
        raise ValueError(f"unsupported geometry type {geometry_type!r}")
    if not isinstance(polygons, list):
        raise ValueError("geometry coordinates are not an array")
    return [
        [simplify_ring(ring, epsilon) for ring in polygon]
        for polygon in polygons
    ]


def main() -> int:
    if len(sys.argv) not in (3, 4):
        print(
            "usage: build-tz-boundaries.py INPUT_GEOJSON OUTPUT_TSV [EPSILON]",
            file=sys.stderr,
        )
        return 2
    input_path = Path(sys.argv[1])
    output_path = Path(sys.argv[2])
    epsilon = float(sys.argv[3]) if len(sys.argv) == 4 else 0.01
    document = json.loads(input_path.read_text(encoding="utf-8"))
    features = document.get("features")
    if document.get("type") != "FeatureCollection" or not isinstance(features, list):
        raise ValueError("input must be a GeoJSON FeatureCollection")

    zones: dict[str, list[list[list[list[float]]]]] = {}
    for feature in features:
        properties = feature.get("properties", {})
        tz_name = properties.get("tzid")
        if not isinstance(tz_name, str) or not tz_name or "\t" in tz_name:
            raise ValueError("feature has an invalid tzid")
        zones.setdefault(tz_name, []).extend(
            normalize_geometry(feature.get("geometry"), epsilon)
        )
    if not zones:
        raise ValueError("input contains no timezone features")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with output_path.open("w", encoding="utf-8", newline="\n") as output:
        for tz_name in sorted(zones):
            geometry = {
                "type": "MultiPolygon",
                "coordinates": zones[tz_name],
            }
            output.write(
                f"{tz_name}\t{json.dumps(geometry, separators=(',', ':'))}\n"
            )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
