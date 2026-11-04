#!/usr/bin/env python3
"""Create/inspect unsigned Agbrio Mac updater archives. Never signs or publishes."""
import argparse
import hashlib
import json
import os
import plistlib
import posixpath
import re
import stat
import struct
import tarfile
import unicodedata
import zipfile
from pathlib import Path, PurePosixPath

REPO = "geoffrey1111/agbrio"
APP = "Agbrio.app"
IDENTIFIER = "com.geoffrey.aiworkrouter"
MAX_ARCHIVE = 600 * 1024 * 1024
MAX_EXPANDED = 2 * 1024 * 1024 * 1024
SCHEMA = "agbrio-macos-update-v1"


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def version_ok(value):
    return bool(re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?", value))


def member_path(name):
    require(isinstance(name, str) and "\\" not in name and ":" not in name and "\0" not in name, "invalid member path")
    value = name.rstrip("/")
    parts = value.split("/")
    require(parts and parts[0] == APP and all(p and p not in (".", "..") for p in parts), "member outside app")
    require(not any(ord(c) < 32 for c in value), "control character in path")
    return value


def link_target(name, target):
    require(target and not target.startswith("/") and "\\" not in target and ":" not in target and "\0" not in target, "invalid link")
    require(not any(ord(c) < 32 for c in target), "control character in link")
    resolved = posixpath.normpath(posixpath.join(posixpath.dirname(name), target))
    require(resolved == APP or resolved.startswith(APP + "/"), "escaping link")
    return resolved


def arm64(data):
    require(len(data) >= 32, "truncated Mach-O")
    magic, cpu = struct.unpack_from("<II", data)
    require(magic == 0xFEEDFACF and cpu == 0x0100000C, "required ARM64 Mach-O missing")
    commands, size = struct.unpack_from("<II", data, 16)
    require(commands <= 65536 and size <= len(data) - 32, "invalid Mach-O commands")
    cursor, minimum = 32, None
    for _ in range(commands):
        require(cursor + 8 <= 32 + size, "truncated load command")
        command, length = struct.unpack_from("<II", data, cursor)
        require(length >= 8 and cursor + length <= 32 + size, "invalid load command")
        if command == 0x32:
            require(length >= 24, "invalid build version")
            platform, value = struct.unpack_from("<II", data, cursor + 8)
            require(platform == 1, "non-macOS executable")
            minimum = value
        elif command == 0x24:
            require(length >= 16, "invalid minimum version")
            minimum = struct.unpack_from("<I", data, cursor + 8)[0]
        cursor += length
    require(cursor == 32 + size and minimum is not None and minimum <= 0x000D0500, "executable requires newer than macOS13.5")


def resolve_link(name, paths):
    seen = set()
    for _ in range(100):
        require(name not in seen, "symlink cycle")
        seen.add(name)
        parts = name.split("/")
        for count in range(1, len(parts) + 1):
            prefix = "/".join(parts[:count])
            row = paths.get(prefix, {})
            if row.get("type") == "symlink":
                name = link_target(prefix, row["target"])
                if count < len(parts):
                    name = posixpath.join(name, *parts[count:])
                break
        else:
            require(name in paths, "dangling symlink")
            return name
    raise ValueError("symlink chain too deep")


def inspect(archive, version, source):
    require(version_ok(version), "invalid version")
    require(bool(re.fullmatch(r"[0-9a-f]{40}", source)), "full source SHA required")
    archive = Path(archive)
    require(archive.name == f"Agbrio_{version}_aarch64.app.tar.gz", "archive filename/version mismatch")
    require(archive.is_file() and not archive.is_symlink() and 0 < archive.stat().st_size <= MAX_ARCHIVE, "archive size/path invalid")
    records, aliases, paths, content = [], {}, {}, {}
    expanded = 0
    with tarfile.open(archive, "r:gz") as reader:
        for entry in reader:
            require(len(records) < 100000, "too many entries")
            name = member_path(entry.name)
            folded = unicodedata.normalize("NFC", name).casefold()
            require(folded not in aliases, "duplicate or case/Unicode alias")
            aliases[folded] = name
            require(not entry.mode & 0o7000, "privileged mode bits")
            require(entry.uid == 0 and entry.gid == 0, "archive ownership must be normalized")
            row = {"path": name, "mode": entry.mode & 0o777}
            if entry.isdir():
                row["type"] = "directory"
            elif entry.issym():
                row.update(type="symlink", target=entry.linkname)
                link_target(name, entry.linkname)
            elif entry.isfile():
                require(0 <= entry.size <= MAX_EXPANDED, "member size invalid")
                expanded += entry.size
                require(expanded <= MAX_EXPANDED, "expanded archive too large")
                stream = reader.extractfile(entry)
                digest = hashlib.sha256()
                prefix = bytearray()
                read = 0
                for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                    read += len(chunk)
                    digest.update(chunk)
                    if len(prefix) < 1024 * 1024:
                        prefix.extend(chunk[:1024 * 1024 - len(prefix)])
                require(read == entry.size, "truncated member")
                row.update(type="file", size=read, sha256=digest.hexdigest())
                if name in (f"{APP}/Contents/Info.plist", f"{APP}/Contents/MacOS/ai-work-router", f"{APP}/Contents/MacOS/browser-executor-node"):
                    content[name] = bytes(prefix)
                    if name.endswith("Info.plist"):
                        require(read <= 1024 * 1024, "oversized Info.plist")
            else:
                raise ValueError("special files and hard links are not supported")
            require(not name.startswith(f"{APP}/Contents/Resources/pi-adapter"), "Mac must omit Pi")
            paths[name] = row
            records.append(row)
    require(paths.get(APP, {}).get("type") == "directory", "app root missing")
    for name, row in paths.items():
        for parent in PurePosixPath(name).parents:
            if str(parent) != ".":
                require(paths.get(str(parent), {}).get("type") == "directory", "missing/non-directory parent")
        if row["type"] == "symlink":
            resolve_link(name, paths)
    info = plistlib.loads(content.get(f"{APP}/Contents/Info.plist", b""))
    require(info.get("CFBundleIdentifier") == IDENTIFIER, "bundle identity mismatch")
    require(info.get("CFBundleShortVersionString") == version and info.get("CFBundleVersion") == version, "bundle version mismatch")
    require(info.get("CFBundleExecutable") == "ai-work-router", "executable identity mismatch")
    require(info.get("LSMinimumSystemVersion") in ("13.5", "13.5.0"), "bundle minimum-system mismatch")
    for filename in ("ai-work-router", "browser-executor-node"):
        name = f"{APP}/Contents/MacOS/{filename}"
        require(paths.get(name, {}).get("type") == "file" and paths[name]["mode"] & 0o111, "required executable missing")
        arm64(content.get(name, b""))
    return {"schema": SCHEMA, "repository": REPO, "sourceCommit": source, "platform": "darwin-aarch64", "version": version,
            "archive": {"name": archive.name, "size": archive.stat().st_size, "sha256": sha(archive.read_bytes())},
            "files": sorted(records, key=lambda r: r["path"])}


def pack(app, output, version, source):
    app, output = Path(app), Path(output)
    require(version_ok(version) and bool(re.fullmatch(r"[0-9a-f]{40}", source)), "valid version/source required")
    require(app.name == APP and app.is_dir() and not app.is_symlink(), "owned app bundle required")
    require(not output.exists(), "preserve existing archive")
    output.parent.mkdir(parents=True, exist_ok=True)
    def normalize(entry):
        member_path(entry.name)
        entry.uid = entry.gid = 0
        entry.uname = entry.gname = ""
        entry.mtime = 0
        entry.pax_headers = {}
        return entry
    created = False
    identity = None
    try:
        with output.open("xb") as stream:
            created = True
            metadata = os.fstat(stream.fileno())
            identity = (metadata.st_dev, metadata.st_ino)
            with tarfile.open(fileobj=stream, mode="w:gz", dereference=False, format=tarfile.PAX_FORMAT) as writer:
                writer.add(app, arcname=APP, recursive=True, filter=normalize)
        return inspect(output, version, source)
    except BaseException:
        # Exact newly created output only; never remove the source app.
        if created and output.exists() and not output.is_symlink():
            metadata = output.stat()
            if identity == (metadata.st_dev, metadata.st_ino):
                output.unlink()
        raise


def verify(archive, manifest, version, source, expected_sha):
    require(bool(re.fullmatch(r"[0-9a-f]{64}", expected_sha)), "expected archive SHA required")
    data = Path(manifest).read_bytes()
    require(len(data) <= 32 * 1024 * 1024, "oversized manifest")
    actual = inspect(archive, version, source)
    require(actual["archive"]["sha256"] == expected_sha, "archive hash mismatch")
    require(json.loads(data) == actual, "file manifest mismatch")
    return actual


def unpack_artifact(zip_path, output, version, source, expected_zip_sha):
    zip_path, output = Path(zip_path), Path(output)
    require(version_ok(version) and re.fullmatch(r"[0-9a-f]{40}", source), "invalid version/source")
    require(re.fullmatch(r"[0-9a-f]{64}", expected_zip_sha), "expected artifact ZIP hash required")
    require(zip_path.is_file() and not zip_path.is_symlink() and zip_path.stat().st_size <= MAX_ARCHIVE + 32 * 1024 * 1024, "invalid artifact ZIP")
    require(sha(zip_path.read_bytes()) == expected_zip_sha, "artifact ZIP digest mismatch")
    require(not output.exists() and output.parent.is_dir(), "new owned output directory required")
    for parent in [output.parent, *output.parent.parents]:
        require(not parent.is_symlink(), "linked output ancestor")
    archive_name = f"Agbrio_{version}_aarch64.app.tar.gz"
    required = {archive_name, "MACOS_UPDATE_MANIFEST.json"}
    allowed = required | {"SHA256SUMS.txt"}
    with zipfile.ZipFile(zip_path) as reader:
        rows = reader.infolist()
        names = [row.filename for row in rows]
        require(len(names) == len(set(names)) and required.issubset(names) and set(names).issubset(allowed), "artifact members mismatch")
        for row in rows:
            require(not row.is_dir() and stat.S_IFMT(row.external_attr >> 16) != stat.S_IFLNK, "linked artifact member")
            limit = MAX_ARCHIVE if row.filename == archive_name else 32 * 1024 * 1024
            require(0 <= row.file_size <= limit, "artifact member too large")
        output.mkdir(mode=0o700)
        # On failure preserve this exact partial staging for diagnosis. Never
        # recurse over a caller directory or overwrite a previous candidate.
        for row in rows:
            with reader.open(row) as src, (output / row.filename).open("xb") as dst:
                count = 0
                for chunk in iter(lambda: src.read(1024 * 1024), b""):
                    count += len(chunk)
                    require(count <= row.file_size, "expanded ZIP mismatch")
                    dst.write(chunk)
                require(count == row.file_size, "truncated ZIP member")
    manifest = json.loads((output / "MACOS_UPDATE_MANIFEST.json").read_text(encoding="utf-8"))
    return verify(output / archive_name, output / "MACOS_UPDATE_MANIFEST.json", version, source, manifest["archive"]["sha256"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["pack", "verify", "unpack"])
    parser.add_argument("--app")
    parser.add_argument("--archive", required=True)
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--sha256")
    parser.add_argument("--zip")
    args = parser.parse_args()
    if args.action == "pack":
        require(args.app and not Path(args.manifest).exists(), "app/new manifest required")
        result = pack(args.app, args.archive, args.version, args.source)
        with open(args.manifest, "x", encoding="utf-8") as stream:
            json.dump(result, stream, ensure_ascii=False, sort_keys=True, indent=2)
            stream.write("\n")
    elif args.action == "unpack":
        output = Path(args.archive).parent
        require(Path(args.manifest) == output / "MACOS_UPDATE_MANIFEST.json", "manifest output mismatch")
        result = unpack_artifact(args.zip, output, args.version, args.source, args.sha256)
    else:
        result = verify(args.archive, args.manifest, args.version, args.source, args.sha256)
    print(json.dumps({"result": "MAC_UNSIGNED_UPDATE_PACKAGE_VALID", "version": result["version"], "source": result["sourceCommit"], "sha256": result["archive"]["sha256"], "signed": False, "published": False}))


if __name__ == "__main__":
    main()
