"""Manual CI audit of the fixed official Mac archive; no accounts or profiles."""
import base64
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import plistlib
import posixpath
import shutil
import stat
import subprocess
import sys
import tempfile
import urllib.request
import urllib.parse
import zipfile

SNAPSHOT = "1707745"
VERSION = "157.0.8079.0"
OBJECT = f"Mac_Arm/{SNAPSHOT}/chrome-mac.zip"
SOURCE = f"https://storage.googleapis.com/chromium-browser-snapshots/{OBJECT}"


def digest(path, algorithm="sha256"):
    result = hashlib.new(algorithm)
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.digest()


def main():
    if sys.platform != "darwin" or os.uname().machine != "arm64":
        raise RuntimeError("MACOS_ARM64_REQUIRED")
    parent = Path(os.environ["RUNNER_TEMP"]).resolve(strict=True)
    output = parent / "agbrio-chromium-audit"
    output.mkdir(mode=0o700)  # never reuse another run's output
    work = Path(tempfile.mkdtemp(prefix="work-", dir=output))
    try:
        metadata_url = "https://www.googleapis.com/storage/v1/b/chromium-browser-snapshots/o/" + urllib.parse.quote(OBJECT, safe="")
        with urllib.request.urlopen(metadata_url, timeout=30) as response:
            metadata = json.load(response)
        if metadata.get("name") != OBJECT or not metadata.get("generation", "").isdigit():
            raise RuntimeError("OFFICIAL_OBJECT_IDENTITY_INVALID")
        archive = work / "chrome.zip"
        pinned_url = SOURCE + "?generation=" + metadata["generation"]
        with urllib.request.urlopen(pinned_url, timeout=120) as response, archive.open("xb") as target:
            size = 0
            while chunk := response.read(1024 * 1024):
                size += len(chunk)
                if size > 1200 * 1024 * 1024:
                    raise RuntimeError("ARCHIVE_TOO_LARGE")
                target.write(chunk)
        if archive.stat().st_size != int(metadata["size"]) or base64.b64encode(digest(archive, "md5")).decode() != metadata["md5Hash"]:
            raise RuntimeError("OFFICIAL_OBJECT_INTEGRITY_MISMATCH")
        with zipfile.ZipFile(archive) as zipped:
            expanded = 0
            for info in zipped.infolist():
                name = PurePosixPath(info.filename)
                if name.is_absolute() or ".." in name.parts or "\\" in info.filename or not name.parts or name.parts[0] != "chrome-mac":
                    raise RuntimeError("ARCHIVE_PATH_INVALID")
                expanded += info.file_size
                if expanded > 3 * 1024 * 1024 * 1024:
                    raise RuntimeError("EXPANDED_ARCHIVE_TOO_LARGE")
                if stat.S_ISLNK(info.external_attr >> 16):
                    if info.file_size > 4096:
                        raise RuntimeError("ARCHIVE_LINK_INVALID")
                    link = zipped.read(info).decode("utf8")
                    destination = posixpath.normpath(str(name.parent / link))
                    if link.startswith("/") or not destination.startswith("chrome-mac/"):
                        raise RuntimeError("ARCHIVE_LINK_ESCAPES_ROOT")
        extracted = work / "extracted"
        subprocess.run(["/usr/bin/ditto", "-x", "-k", str(archive), str(extracted)], check=True, timeout=120)
        root = extracted / "chrome-mac"
        binary = root / "Chromium.app/Contents/MacOS/Chromium"
        with (root / "Chromium.app/Contents/Info.plist").open("rb") as stream:
            info = plistlib.load(stream)
        if info.get("CFBundleShortVersionString") != VERSION or not binary.stat().st_mode & 0o111:
            raise RuntimeError("CHROMIUM_VERSION_OR_EXECUTABLE_INVALID")
        subprocess.run(["/usr/bin/lipo", str(binary), "-verify_arch", "arm64"], check=True, timeout=10)
        files, links, modes = {}, {}, {}
        for folder, directories, names in os.walk(root, followlinks=False):
            for name in directories + names:
                path = Path(folder) / name
                relative = path.relative_to(root).as_posix()
                mode = path.lstat().st_mode
                if stat.S_ISLNK(mode):
                    if not path.resolve(strict=True).is_relative_to(root.resolve()):
                        raise RuntimeError("EXTRACTED_LINK_ESCAPES_ROOT")
                    links[relative] = os.readlink(path)
                elif stat.S_ISREG(mode):
                    files[relative] = digest(path).hex()
                    modes[relative] = stat.S_IMODE(mode)
                elif not stat.S_ISDIR(mode):
                    raise RuntimeError("EXTRACTED_SPECIAL_FILE_INVALID")
        executable = binary.relative_to(root).as_posix()
        manifest = {"schema": "AIWR_CHROMIUM_PACKAGE_V2", "platform": "darwin-arm64", "snapshot": SNAPSHOT,
                    "productName": "Chromium", "version": VERSION, "source": SOURCE,
                    "archiveSha256": digest(archive).hex(), "executable": executable,
                    "executableSha256": files[executable],
                    "hashProvenance": "CI-derived from the generation-pinned official snapshot and upstream object MD5; not an upstream signature",
                    "files": dict(sorted(files.items())), "links": dict(sorted(links.items())), "modes": dict(sorted(modes.items()))}
        (output / "chromium-manifest-macos.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf8")
        receipt = {"snapshot": SNAPSHOT, "version": VERSION, "source": SOURCE,
                   "generation": metadata["generation"], "archiveBytes": archive.stat().st_size,
                   "archiveSha256": manifest["archiveSha256"], "officialMd5": metadata["md5Hash"],
                   "files": len(files), "links": len(links), "arm64": True,
                   "browserLaunched": False, "profileOrAccountUsed": False}
        (output / "audit-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf8")
        print(json.dumps(receipt))
    finally:
        # Only this newly-created work directory; retain the two final manifests.
        if work.parent == output and work.name.startswith("work-"):
            shutil.rmtree(work)


if __name__ == "__main__":
    main()
