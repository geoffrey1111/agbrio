"""Collect notices from the actual locked, installed build dependencies.

Run after npm ci in both package roots and cargo metadata --locked. No secrets,
runtime records or absolute build-machine paths are written to the inventory.
"""
from pathlib import Path
import json, shutil, subprocess, urllib.request, re

ROOT = Path(__file__).resolve().parent.parent
DEST = ROOT / 'third-party'
DEST.mkdir(exist_ok=True)
rows = []

def notices(kind, name, version, license_id, directory, source):
    if not license_id:
        raise RuntimeError(f'Missing declared license: {kind}/{name}@{version}')
    relative = f'{kind}/{name.replace("/", "_").replace("@", "")}-{version}'
    found = []
    for file in directory.iterdir():
        if file.is_file() and file.name.lower().startswith(('license', 'licence', 'copying', 'notice', 'copyright')):
            target = DEST / relative / file.name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(file, target)
            found.append(target.relative_to(ROOT).as_posix())
    if not found:
        # Some published artifacts omit their license file. Retain attribution
        # from that exact artifact and the complete standard license terms.
        attribution = directory/('Cargo.toml.orig' if kind=='cargo' else 'package.json')
        if not attribution.exists(): attribution=directory/'Cargo.toml'
        target=DEST/relative/'UPSTREAM-ATTRIBUTION.txt';target.parent.mkdir(parents=True,exist_ok=True)
        target.write_text(f'Upstream package: {name}@{version}\nDeclared license: {license_id}\nExact source: {source}\n\n'+attribution.read_text(encoding='utf-8'),encoding='utf-8')
        found.append(target.relative_to(ROOT).as_posix())
        known=['MIT','Apache-2.0','MPL-2.0','BSD-3-Clause','ISC']
        for spdx in known:
            if spdx not in license_id: continue
            terms=DEST/'licenses'/f'{spdx}.txt';terms.parent.mkdir(exist_ok=True)
            if not terms.exists():
                with urllib.request.urlopen('https://raw.githubusercontent.com/spdx/license-list-data/main/text/'+spdx+'.txt',timeout=25) as response:terms.write_bytes(response.read())
            found.append(terms.relative_to(ROOT).as_posix())
        if 'MPL-' in license_id:
            # Make covered source obtainable even if the upstream link changes.
            src=DEST/relative/'source'
            shutil.copytree(directory,src,dirs_exist_ok=True,ignore=shutil.ignore_patterns('.cargo-checksum.json','.cargo-ok'))
    rows.append(dict(ecosystem=kind,name=name,version=version,license=license_id,source=source,notices=found))

for folder in [ROOT, ROOT/'tools/isolated-browser-executor']:
    lock = json.loads((folder/'package-lock.json').read_text(encoding='utf-8'))
    for relative, row in lock['packages'].items():
        if not relative or 'node_modules/' not in relative:
            continue
        directory = folder/relative
        if not directory.is_dir():
            continue # optional package for another OS
        package = json.loads((directory/'package.json').read_text(encoding='utf-8'))
        if any(r['ecosystem']=='npm' and r['name']==package['name'] and r['version']==package['version'] for r in rows):
            continue
        notices('npm',package['name'],package['version'],package.get('license') or row.get('license'),directory,row.get('resolved','https://www.npmjs.com/package/'+package['name']))

metadata = json.loads(subprocess.check_output(['cargo','metadata','--manifest-path',str(ROOT/'src-tauri/Cargo.toml'),'--locked','--format-version','1','--filter-platform','x86_64-pc-windows-msvc'],cwd=ROOT,text=True,encoding='utf-8'))
for package in metadata['packages']:
    if not package['source']:
        continue # Agbrio itself
    notices('cargo',package['name'],package['version'],package['license'],Path(package['manifest_path']).parent,'https://crates.io/api/v1/crates/'+package['name']+'/'+package['version']+'/download')

(DEST/'inventory.json').write_text(json.dumps(sorted(rows,key=lambda r:(r['ecosystem'],r['name'],r['version'])),ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(dict(dependencies=len(rows),mplSourceLinks=sum('MPL-' in r['license'] for r in rows))))
