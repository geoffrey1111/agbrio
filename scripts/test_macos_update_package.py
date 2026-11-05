import importlib.util
import io
import json
import plistlib
import struct
import tarfile
import tempfile
import unittest
import zipfile
from unittest.mock import patch
from pathlib import Path

spec = importlib.util.spec_from_file_location("mac_package", Path(__file__).with_name("macos-update-package.py"))
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
VERSION = "0.1.1"
SOURCE = "a" * 40


class MacUpdatePackageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.archive = self.root / f"Agbrio_{VERSION}_aarch64.app.tar.gz"

    def write(self, extra=(), identity=package.IDENTIFIER, version=VERSION, cpu=0x0100000C):
        rows = [("Agbrio.app", "d", b""), ("Agbrio.app/Contents", "d", b""), ("Agbrio.app/Contents/MacOS", "d", b"")]
        info = plistlib.dumps({"CFBundleIdentifier": identity, "CFBundleVersion": version, "CFBundleShortVersionString": version, "CFBundleExecutable": "ai-work-router", "LSMinimumSystemVersion": "13.5"})
        binary = struct.pack("<IIIIIIII", 0xFEEDFACF, cpu, 0, 2, 1, 24, 0, 0) + struct.pack("<IIIIII", 0x32, 24, 1, 0x000D0500, 0x001A0000, 0)
        rows += [("Agbrio.app/Contents/Info.plist", "f", info), ("Agbrio.app/Contents/MacOS/ai-work-router", "x", binary), ("Agbrio.app/Contents/MacOS/browser-executor-node", "x", binary)]
        with tarfile.open(self.archive, "w:gz") as writer:
            for name, kind, value in rows + list(extra):
                entry = tarfile.TarInfo(name)
                entry.mode = 0o755 if kind in ("d", "x") else 0o644
                if kind == "d":
                    entry.type = tarfile.DIRTYPE
                elif kind == "s":
                    entry.type = tarfile.SYMTYPE
                    entry.linkname = value
                elif kind == "h":
                    entry.type = tarfile.LNKTYPE
                    entry.linkname = value
                else:
                    entry.size = len(value)
                writer.addfile(entry, io.BytesIO(value) if kind in ("f", "x") else None)

    def test_real_tar_inspection_and_closed_manifest_reject_changed_bytes_or_metadata(self):
        self.write()
        result = package.inspect(self.archive, VERSION, SOURCE)
        manifest = self.root / "manifest.json"
        manifest.write_text(json.dumps(result), encoding="utf-8")
        self.assertEqual(package.verify(self.archive, manifest, VERSION, SOURCE, result["archive"]["sha256"]), result)
        with self.assertRaises(ValueError):
            package.verify(self.archive, manifest, VERSION, SOURCE, "b" * 64)
        result["files"][0]["mode"] = 0
        manifest.write_text(json.dumps(result), encoding="utf-8")
        with self.assertRaises(ValueError):
            package.verify(self.archive, manifest, VERSION, SOURCE, package.sha(self.archive.read_bytes()))

    def test_path_traversal_absolute_paths_and_case_aliases_are_rejected(self):
        for name in ["../other", "/Agbrio.app/evil", "Agbrio.app/../evil", "Agbrio.app/Contents/Info.plist", "Agbrio.app/contents/INFO.plist", "Agbrio.app/C:/evil"]:
            with self.subTest(name=name):
                self.write([(name, "f", b"bad")])
                with self.assertRaises(ValueError):
                    package.inspect(self.archive, VERSION, SOURCE)

    def test_framework_relative_symlink_chain_is_allowed(self):
        self.write([("Agbrio.app/Contents/Frameworks", "d", b""), ("Agbrio.app/Contents/Frameworks/F.framework", "d", b""),
                    ("Agbrio.app/Contents/Frameworks/F.framework/Versions", "d", b""), ("Agbrio.app/Contents/Frameworks/F.framework/Versions/A", "d", b""),
                    ("Agbrio.app/Contents/Frameworks/F.framework/Versions/A/Resources", "d", b""),
                    ("Agbrio.app/Contents/Frameworks/F.framework/Versions/Current", "s", "A"), ("Agbrio.app/Contents/Frameworks/F.framework/Resources", "s", "Versions/Current/Resources")])
        self.assertTrue(package.inspect(self.archive, VERSION, SOURCE)["files"])

    def test_release_path_policy_rejects_non_ascii_and_writable_members(self):
        for rows in [[("Agbrio.app/中文", "f", b"bad")], [("Agbrio.app/link", "s", "目標")]]:
            self.write(rows)
            with self.assertRaises(ValueError):
                package.inspect(self.archive, VERSION, SOURCE)
        self.write()
        with tarfile.open(self.archive, "r:gz") as reader:
            rows = [(entry, reader.extractfile(entry).read() if entry.isfile() else None) for entry in reader]
        rows[0][0].mode = 0o777
        with tarfile.open(self.archive, "w:gz") as writer:
            for entry, data in rows:
                writer.addfile(entry, io.BytesIO(data) if data is not None else None)
        with self.assertRaises(ValueError):
            package.inspect(self.archive, VERSION, SOURCE)

    def test_escaping_dangling_and_parent_symlinks_or_hardlinks_are_rejected(self):
        for rows in [[("Agbrio.app/escape", "s", "../../other")], [("Agbrio.app/missing", "s", "absent")],
                     [("Agbrio.app/alias", "s", "Contents"), ("Agbrio.app/alias/extra", "f", b"bad")], [("Agbrio.app/link", "h", "Agbrio.app/Contents/Info.plist")]]:
            self.write(rows)
            with self.assertRaises(ValueError):
                package.inspect(self.archive, VERSION, SOURCE)

    def test_bundle_identity_version_and_arm64_are_required(self):
        for kwargs in [{"identity": "another.app"}, {"version": "9.0.0"}, {"cpu": 0x01000007}]:
            self.write(**kwargs)
            with self.assertRaises(ValueError):
                package.inspect(self.archive, VERSION, SOURCE)

    def test_mac_archive_never_accepts_pi_resources(self):
        self.write([("Agbrio.app/Contents/Resources", "d", b""), ("Agbrio.app/Contents/Resources/pi-adapter", "d", b"")])
        with self.assertRaises(ValueError):
            package.inspect(self.archive, VERSION, SOURCE)

    def test_pack_never_overwrites_an_existing_output_or_accepts_invalid_source(self):
        app = self.root / "Agbrio.app"
        app.mkdir()
        self.archive.write_bytes(b"keep")
        with self.assertRaises(ValueError):
            package.pack(app, self.archive, VERSION, SOURCE)
        self.assertEqual(self.archive.read_bytes(), b"keep")
        with self.assertRaises(ValueError):
            package.pack(app, self.root / "new.tar.gz", VERSION, "not-a-sha")

    def test_artifact_zip_digest_members_and_new_staging_are_verified(self):
        self.write()
        metadata = package.inspect(self.archive, VERSION, SOURCE)
        zipped = self.root / "artifact.zip"
        with zipfile.ZipFile(zipped, "w") as writer:
            writer.write(self.archive, self.archive.name)
            writer.writestr("MACOS_UPDATE_MANIFEST.json", json.dumps(metadata))
        expected = package.sha(zipped.read_bytes())
        out = self.root / "new-owned-staging"
        self.assertEqual(package.unpack_artifact(zipped, out, VERSION, SOURCE, expected), metadata)
        with self.assertRaises(ValueError):
            package.unpack_artifact(zipped, out, VERSION, SOURCE, expected)
        with self.assertRaises(ValueError):
            package.unpack_artifact(zipped, self.root / "bad-digest", VERSION, SOURCE, "b" * 64)
        self.assertFalse((self.root / "bad-digest").exists())

    def test_artifact_zip_traversal_is_rejected_before_staging(self):
        zipped = self.root / "bad.zip"
        with zipfile.ZipFile(zipped, "w") as writer:
            writer.writestr("../outside", b"bad")
        out = self.root / "rejected"
        with self.assertRaises(ValueError):
            package.unpack_artifact(zipped, out, VERSION, SOURCE, package.sha(zipped.read_bytes()))
        self.assertFalse(out.exists())

    def test_pack_failed_exclusive_create_does_not_delete_another_writer_output(self):
        app = self.root / "Agbrio.app"
        app.mkdir()
        original = Path.open
        def race(path, mode="r", *args, **kwargs):
            if path == self.archive and mode == "xb":
                with original(path, "wb") as stream:
                    stream.write(b"foreign writer keep")
                raise FileExistsError("fixture race")
            return original(path, mode, *args, **kwargs)
        with patch.object(Path, "open", race):
            with self.assertRaises(FileExistsError):
                package.pack(app, self.archive, VERSION, SOURCE)
        self.assertEqual(self.archive.read_bytes(), b"foreign writer keep")

    def test_macho_newer_minimum_system_cannot_hide_behind_bundle_metadata(self):
        binary = struct.pack("<IIIIIIII", 0xFEEDFACF, 0x0100000C, 0, 2, 1, 24, 0, 0) + struct.pack("<IIIIII", 0x32, 24, 1, 0x000E0000, 0x001A0000, 0)
        with self.assertRaises(ValueError):
            package.arm64(binary)


if __name__ == "__main__":
    unittest.main()
