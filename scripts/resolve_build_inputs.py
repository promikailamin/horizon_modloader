#!/usr/bin/env python3
"""Resolve .hnmod inputs for the Build .hmmod workflow.

The workflow accepts repository paths and public URLs. This script copies or
downloads every bundle, reads the Horizon/mixins pins, rejects incompatible
combinations early, and writes the resolved values to ``$GITHUB_OUTPUT``.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import sys
import urllib.parse
import urllib.request
import uuid
import zipfile
from pathlib import Path

MAX_BUNDLE_BYTES = 128 * 1024 * 1024
MAX_MANIFEST_BYTES = 1024 * 1024
HEX = re.compile(r"^[0-9a-fA-F]{7,}$")


def fail(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


def split_refs(value: str) -> list[str]:
    refs = [part for part in re.split(r"[\s,]+", value.strip()) if part]
    if not refs:
        fail("mods input is empty; give at least one .hnmod path or URL")
    return refs


def local_path(ref: str) -> Path:
    path = Path(ref).expanduser()
    if not path.is_absolute():
        path = Path.cwd() / path
    return path.resolve()


def url_filename(url: str) -> str:
    name = Path(urllib.parse.unquote(urllib.parse.urlparse(url).path)).name
    return name or "download.hnmod"


def download(url: str, dest: Path) -> None:
    print(f"downloading {url}")
    request = urllib.request.Request(
        url, headers={"User-Agent": "hmloader-build-workflow"}
    )
    try:
        with urllib.request.urlopen(request, timeout=120) as response:
            with dest.open("wb") as output:
                total = 0
                while True:
                    chunk = response.read(1024 * 1024)
                    if not chunk:
                        break
                    total += len(chunk)
                    if total > MAX_BUNDLE_BYTES:
                        fail(f"{url} is larger than 128 MiB")
                    output.write(chunk)
    except SystemExit:
        raise
    except Exception as exc:  # noqa: BLE001 - report the URL and the real error
        fail(f"cannot download {url}: {exc}")
    if dest.stat().st_size == 0:
        fail(f"downloaded file is empty: {url}")


def read_manifest(path: Path) -> dict:
    try:
        with zipfile.ZipFile(path) as archive:
            try:
                info = archive.getinfo("horizon-native.json")
            except KeyError:
                fail(f"{path} has no horizon-native.json")
            if info.file_size > MAX_MANIFEST_BYTES:
                fail(f"{path}: horizon-native.json is unreasonably large")
            raw = archive.read("horizon-native.json")
    except zipfile.BadZipFile as exc:
        fail(f"{path} is not a .hnmod zip archive: {exc}")
    try:
        manifest = json.loads(raw)
    except json.JSONDecodeError as exc:
        fail(f"{path}: horizon-native.json is not valid JSON: {exc}")
    if not isinstance(manifest, dict):
        fail(f"{path}: horizon-native.json must be an object")
    return manifest


def require_text(manifest: dict, key: str, path: Path) -> str:
    value = manifest.get(key)
    if not isinstance(value, str) or not value.strip():
        fail(f"{path}: horizon-native.json has no {key}")
    return value.strip()


def optional_text(manifest: dict, key: str) -> str:
    value = manifest.get(key)
    return value.strip() if isinstance(value, str) else ""


def set_output(name: str, value: str) -> None:
    target = os.environ.get("GITHUB_OUTPUT")
    if not target:
        return
    with open(target, "a", encoding="utf-8") as output:
        if "\n" in value:
            delimiter = f"__hmloader_{uuid.uuid4().hex}__"
            output.write(f"{name}<<{delimiter}\n{value}\n{delimiter}\n")
        else:
            output.write(f"{name}={value}\n")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mods", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--mixins-revision", default="")
    parser.add_argument("--horizon-version", default="")
    parser.add_argument("--allow-mismatch", default="false")
    parser.add_argument("--inputs-dir", default=".agent-work/tmp/build-inputs")
    parser.add_argument(
        "--mods-file", default=".agent-work/tmp/build-inputs/mods.txt"
    )
    args = parser.parse_args()

    allow_mismatch = args.allow_mismatch.strip().lower() in {
        "1",
        "true",
        "yes",
        "on",
    }

    out = args.out.strip()
    if not out.lower().endswith(".hmmod"):
        fail("out must end with .hmmod")
    if out in {".", ".."} or "/" in out or "\\" in out:
        fail("out must be a file name, not a path")

    refs = split_refs(args.mods)
    inputs_dir = Path(args.inputs_dir)
    if inputs_dir.exists():
        shutil.rmtree(inputs_dir)
    inputs_dir.mkdir(parents=True, exist_ok=True)

    mods: list[Path] = []
    used_names: set[str] = set()
    for index, ref in enumerate(refs, 1):
        if re.match(r"^https?://", ref, re.IGNORECASE):
            name = url_filename(ref)
            if not name.lower().endswith(".hnmod"):
                name += ".hnmod"
            dest = inputs_dir / name
            if name.lower() in used_names or dest.exists():
                dest = inputs_dir / f"{index:02d}-{name}"
            download(ref, dest)
        else:
            source = local_path(ref)
            if not source.is_file():
                fail(f"mod file not found: {ref} ({source})")
            dest = inputs_dir / source.name
            if source.name.lower() in used_names or dest.exists():
                dest = inputs_dir / f"{index:02d}-{source.name}"
            shutil.copyfile(source, dest)
        used_names.add(dest.name.lower())
        mods.append(dest.resolve())

    entries = []
    for path in mods:
        manifest = read_manifest(path)
        mod_id = require_text(manifest, "id", path)
        version = require_text(manifest, "version", path)
        horizon = require_text(manifest, "horizon", path)
        if not HEX.fullmatch(horizon):
            fail(f"{path}: horizon is not a hexadecimal commit id")
        mixins = optional_text(manifest, "mixins")
        if mixins and not HEX.fullmatch(mixins):
            fail(f"{path}: mixins is not a hexadecimal commit id")
        entries.append(
            {
                "path": path,
                "id": mod_id,
                "version": version,
                "horizon": horizon,
                "mixins": mixins,
                "game": optional_text(manifest, "game"),
            }
        )

    seen_ids: set[str] = set()
    for entry in entries:
        if entry["id"] in seen_ids:
            fail(f"two bundles have the same mod id: {entry['id']}")
        seen_ids.add(entry["id"])

    horizon = entries[0]["horizon"]
    mismatched = [e for e in entries[1:] if e["horizon"] != horizon]
    if mismatched and not allow_mismatch:
        details = ", ".join(f"{e['id']}={e['horizon']}" for e in mismatched)
        fail(
            "bundles pin different Horizon revisions "
            f"({entries[0]['id']}={horizon}, {details}); "
            "build them separately or set allow_mismatch"
        )

    mixins_revision = args.mixins_revision.strip()
    if mixins_revision and not HEX.fullmatch(mixins_revision):
        fail("mixins_revision is not a hexadecimal commit id")
    if not mixins_revision:
        pins = [e["mixins"] for e in entries if e["mixins"]]
        if pins:
            mixins_revision = pins[0]
            if len(set(pins)) > 1 and not allow_mismatch:
                fail(
                    "bundles pin different horizon-mixins revisions "
                    f"({', '.join(sorted(set(pins)))}); "
                    "set mixins_revision or allow_mismatch"
                )

    horizon_version = args.horizon_version.strip() or entries[0]["game"]

    mods_file = Path(args.mods_file)
    mods_file.parent.mkdir(parents=True, exist_ok=True)
    mods_file.write_text(
        "\n".join(str(path) for path in mods) + "\n", encoding="utf-8"
    )

    summary = "\n".join(
        f"- `{e['id']}` {e['version']} (`.hnmod`: `{e['path'].name}`)"
        for e in entries
    )
    print(f"resolved {len(entries)} mod(s):")
    print(summary)
    print(f"Horizon commit:  {horizon}")
    print(f"mixins commit:   {mixins_revision or '(default branch)'}")
    print(f"output:          {out}")

    set_output("mods_file", str(mods_file.resolve()))
    set_output("horizon_revision", horizon)
    set_output("mixins_revision", mixins_revision)
    set_output("horizon_version", horizon_version)
    set_output("out", out)
    set_output("mod_count", str(len(entries)))
    set_output(
        "mods_summary",
        ", ".join(f"{e['id']} {e['version']}" for e in entries),
    )


if __name__ == "__main__":
    main()
