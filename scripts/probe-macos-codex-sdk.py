"""Manual CI interoperability probe of the public, signed Desktop distribution.
No app UI, account, user profile, thread, turn or model is opened.
"""
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request

SOURCE = "https://persistent.oaistatic.com/codex-app-prod/Codex.dmg"
DOCUMENTATION = "https://learn.chatgpt.com/docs/app"


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for part in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(part)
    return digest.hexdigest()


def run(args, env=None, timeout=30):
    return subprocess.run(args, env=env, capture_output=True, text=True, timeout=timeout)


def main():
    if sys.platform != "darwin" or os.uname().machine != "arm64":
        raise RuntimeError("MACOS_ARM64_REQUIRED")
    parent = Path(os.environ["RUNNER_TEMP"]).resolve(strict=True)
    output = parent / "agbrio-codex-sdk"
    output.mkdir(mode=0o700)
    work = Path(tempfile.mkdtemp(prefix="work-", dir=output))
    mount = work / "mounted"
    mounted = False
    facts = {"source": SOURCE, "documentation": DOCUMENTATION, "appLaunched": False,
             "accountUsed": False, "modelRequests": 0, "state": "INSPECTING"}
    try:
        archive = work / "desktop.dmg"
        with urllib.request.urlopen(SOURCE, timeout=120) as response, archive.open("xb") as target:
            size = 0
            while part := response.read(1024 * 1024):
                size += len(part)
                if size > 2 * 1024 * 1024 * 1024:
                    raise RuntimeError("DESKTOP_ARCHIVE_TOO_LARGE")
                target.write(part)
        facts.update(archiveSha256=sha(archive), archiveBytes=archive.stat().st_size)
        mount.mkdir(mode=0o700)
        subprocess.run(["/usr/bin/hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", str(mount), str(archive)], check=True, capture_output=True, timeout=60)
        mounted = True
        apps = [p for p in mount.iterdir() if p.suffix == ".app" and p.is_dir() and not p.is_symlink()]
        if len(apps) != 1:
            raise RuntimeError("DESKTOP_BUNDLE_AMBIGUOUS")
        app = apps[0]
        with (app / "Contents/Info.plist").open("rb") as stream:
            info = plistlib.load(stream)
        facts["bundle"] = {key: info.get(key) for key in ["CFBundleIdentifier", "CFBundleShortVersionString", "CFBundleVersion", "CFBundleExecutable", "LSMinimumSystemVersion"]}
        verified = run(["/usr/bin/codesign", "--verify", "--deep", "--strict", str(app)])
        detail = run(["/usr/bin/codesign", "--display", "--verbose=4", str(app)])
        team = re.search(r"^TeamIdentifier=([A-Z0-9]{10})$", detail.stderr, re.M)
        publisher = re.search(r"^Authority=Developer ID Application: OpenAI(?: OpCo)?,? (?:L\.L\.C\.|LLC) \(([A-Z0-9]{10})\)$", detail.stderr, re.M)
        assessed = run(["/usr/sbin/spctl", "--assess", "--type", "execute", "--verbose=4", str(app)], timeout=90)
        facts.update(codeSignatureValid=verified.returncode == 0, teamId=team.group(1) if team else None,
                     publisherIsOpenAI=bool(publisher and team and publisher.group(1) == team.group(1)), gatekeeperAccepted=assessed.returncode == 0)
        resources = app / "Contents/Resources"
        asar = resources / "app.asar"
        if asar.is_file():
            data = asar.read_bytes()
            facts["desktopMarkers"] = {"CODEX_APP_SERVER_WS_URL": b"CODEX_APP_SERVER_WS_URL" in data, "unixScheme": b"unix://" in data, "presenceIsNotCompatibilityProof": True}
        native = [p for p in resources.iterdir() if p.is_file() and not p.is_symlink() and p.name.startswith("codex")]
        facts["nativeMembers"] = [{"name": p.name, "bytes": p.stat().st_size, "sha256": sha(p)} for p in sorted(native)]
        if not all([facts["codeSignatureValid"], facts["publisherIsOpenAI"], facts["gatekeeperAccepted"]]):
            raise RuntimeError("DESKTOP_PUBLISHER_UNVERIFIED")
        cli = resources / "codex"
        if not cli.is_file() or cli.is_symlink():
            raise RuntimeError("DESKTOP_NATIVE_CLI_MISSING")
        subprocess.run(["/usr/bin/lipo", str(cli), "-verify_arch", "arm64"], check=True, capture_output=True)
        home = work / "empty-home"
        codex_home = home / ".codex"
        codex_home.mkdir(parents=True, mode=0o700)
        env = {"PATH": "/usr/bin:/bin", "HOME": str(home), "CODEX_HOME": str(codex_home), "LC_ALL": "C"}
        version = run([str(cli), "--version"], env=env)
        if version.returncode != 0 or not re.fullmatch(r"codex-cli [^\s]+\s*", version.stdout):
            raise RuntimeError("DESKTOP_NATIVE_VERSION_UNVERIFIED")
        facts["cliVersion"] = version.stdout.strip()
        help_result = run([str(cli), "app-server", "--help"], env=env)
        facts["appServerFlags"] = {flag: flag in help_result.stdout for flag in ["--listen", "--ws-auth", "--ws-token-sha256", "unix://"]}
        companion = resources / "codex-code-mode-host"
        if companion.is_file() and not companion.is_symlink():
            result = run([os.environ["AGBRIO_PROBE_NODE"], "src-tauri/resources/codex-shared/code-mode-probe.mjs", str(companion)], env=env, timeout=30)
            if result.returncode != 0:
                raise RuntimeError("DESKTOP_CODE_MODE_PROBE_FAILED")
            facts["codeModeProbe"] = json.loads(result.stdout)
        else:
            raise RuntimeError("DESKTOP_CODE_MODE_HOST_MISSING")
        facts["state"] = "SIGNED_SDK_PROBE_PASS"
    except Exception as error:
        facts["state"] = "PROBE_FAILED"
        facts["errorCode"] = str(error) if re.fullmatch(r"[A-Z_]+", str(error)) else "DESKTOP_SDK_PROBE_UNAVAILABLE"
        raise
    finally:
        (output / "probe.json").write_text(json.dumps(facts, indent=2) + "\n", encoding="utf8")
        if mounted:
            subprocess.run(["/usr/bin/hdiutil", "detach", str(mount)], check=True, capture_output=True, timeout=60)
        if work.parent == output and work.name.startswith("work-"):
            shutil.rmtree(work)


if __name__ == "__main__":
    main()
