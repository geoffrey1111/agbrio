// Local candidate metadata only. No network, signing-key access or publication.
import {readFile,writeFile} from 'node:fs/promises';
import {parseArgs} from 'node:util';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
export function platformFragment({version,filename,signature}){
 if(!/^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$/.test(version))throw Error('Valid SemVer required');
 if(filename!==`Agbrio_${version}_aarch64.app.tar.gz`||typeof signature!=='string'||!signature.trim()||signature.length>8192)throw Error('Exact archive and signature required');
 return {version,platforms:{'darwin-aarch64':{signature:signature.trim(),url:`https://github.com/geoffrey1111/agbrio/releases/download/v${version}/${filename}`}}};
}
export function mergeManifest(base,fragment){
 if(base?.version!==fragment.version||!base.platforms?.['windows-x86_64']?.signature||!base.platforms?.['windows-x86_64']?.url)throw Error('Same-version Windows manifest required; never discard Windows entries');
 const windows=base.platforms['windows-x86_64'];
 if(windows.url!==`https://github.com/geoffrey1111/agbrio/releases/download/v${base.version}/Agbrio_${base.version}_x64-setup.exe`||typeof windows.signature!=='string'||!windows.signature.trim()||windows.signature.length>8192)throw Error('Existing Windows platform must retain the pinned repository/version/asset');
 return {...structuredClone(base),platforms:{...structuredClone(base.platforms),...fragment.platforms}};
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href){
 const {values}=parseArgs({options:{version:{type:'string'},archive:{type:'string'},signature:{type:'string'},output:{type:'string'},base:{type:'string'}}});
 const signature=await readFile(values.signature,'utf8');const fragment=platformFragment({version:values.version,filename:values.archive,signature});
 const result=values.base?mergeManifest(JSON.parse(await readFile(values.base,'utf8')),fragment):fragment;
 await writeFile(values.output,JSON.stringify(result,null,2)+'\n',{flag:'wx'});console.log('Local Mac platform candidate prepared; not published.');
}
