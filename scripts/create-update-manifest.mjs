// Generate a signed Windows updater manifest from the actual installer/signature.
// This creates local files only. Publishing always remains an explicit operation.
import {readFile,writeFile,stat} from 'node:fs/promises';
import {basename,resolve} from 'node:path';
import {parseArgs} from 'node:util';
import {pathToFileURL} from 'node:url';
export function manifest({version,filename,signature,notes='',date=new Date().toISOString()}){
 if(!/^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$/.test(version))throw Error('Valid SemVer is required');
 if(filename!==`Agbrio_${version}_x64-setup.exe`||!signature.trim()||signature.length>8192)throw Error('Actual versioned installer/signature required');
 return {version,notes,pub_date:date,platforms:{'windows-x86_64':{signature:signature.trim(),url:`https://github.com/geoffrey1111/agbrio/releases/download/v${version}/${filename}`}}};
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href){
 const {values}=parseArgs({options:{version:{type:'string'},installer:{type:'string'},signature:{type:'string'},notes:{type:'string'},output:{type:'string'}}});
 if(!values.version||!values.installer||!values.output)throw Error('--version, --installer and --output are required');
 const size=(await stat(values.installer)).size;if(!size||size>600*1024*1024)throw Error('Installer size invalid');
 const signature=await readFile(values.signature??`${values.installer}.sig`,'utf8');
 const result=manifest({version:values.version,filename:basename(values.installer),signature,notes:values.notes?await readFile(values.notes,'utf8'):''});
 await writeFile(values.output,JSON.stringify(result,null,2)+'\n',{flag:'wx'});console.log(`Generated updater manifest for ${values.version}; not published.`);
}
