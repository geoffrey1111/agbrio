#!/bin/sh
# Explicit owner-run installation only; no account/model/session operation.
set -eu
[ "$(uname -s)" = Darwin ] || { echo 'CLAUDE_MACOS_REQUIRED' >&2; exit 1; }
found=''
count=0
while read -r pid image; do
  case "$image" in
    */Agbrio.app/Contents/MacOS/ai-work-router)
      found=${image%/Contents/MacOS/ai-work-router}
      count=$((count+1))
      ;;
  esac
done <<EOF
$(/bin/ps -ww -U "$(/usr/bin/id -u)" -o pid=,comm=)
EOF
[ "$count" -eq 1 ] || { echo 'CLAUDE_AGBRIO_RUNNING_APP_AMBIGUOUS' >&2; exit 1; }
/usr/bin/codesign --verify --deep --strict "$found" >/dev/null 2>&1 || { echo 'CLAUDE_AGBRIO_PACKAGE_UNVERIFIED' >&2; exit 1; }
node="$found/Contents/MacOS/browser-executor-node"
[ -x "$node" ] || { echo 'CLAUDE_AGBRIO_NODE_MISSING' >&2; exit 1; }
"$node" - "$found" <<'AGBRIO_CLAUDE_INSTALL'
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process');
try{
 const app=fs.realpathSync(process.argv[2]),pack=fs.realpathSync(path.join(app,'Contents/Resources/claude-code-plugin'));
 const manifest=JSON.parse(fs.readFileSync(path.join(pack,'.claude-plugin/plugin.json'),'utf8'));
 const marketplace=JSON.parse(fs.readFileSync(path.join(pack,'.claude-plugin/marketplace.json'),'utf8'));
 if(!pack.startsWith(app+path.sep)||manifest.name!=='agbrio-bridge'||marketplace.name!=='agbrio-observation')throw Error('CLAUDE_OBSERVATION_PLUGIN_NOT_PACKAGED');
 const root=fs.realpathSync(path.join(process.env.HOME,'Library/Application Support/Claude/claude-code'));
 const versions=fs.readdirSync(root,{withFileTypes:true}).filter(v=>v.isDirectory()&&!v.isSymbolicLink()&&/^\d+\.\d+\.\d+$/.test(v.name)).map(v=>v.name).filter(v=>{const n=v.split('.').map(Number);return n[0]>2||n[0]===2&&(n[1]>1||n[1]===1&&n[2]>=286);}).sort((a,b)=>{const x=a.split('.').map(Number),y=b.split('.').map(Number);return y[0]-x[0]||y[1]-x[1]||y[2]-x[2];});
 let executable;
 for(const version of versions){
  const base=path.join(root,version),directories=[base,...fs.readdirSync(base,{withFileTypes:true}).filter(v=>v.isDirectory()&&!v.isSymbolicLink()).slice(0,128).map(v=>path.join(base,v.name))];
  for(const directory of directories){try{const file=fs.realpathSync(path.join(directory,'claude.app/Contents/MacOS/claude')),meta=fs.statSync(file);if(file.startsWith(root+path.sep)&&meta.isFile()&&(meta.mode&0o111)){executable=file;break;}}catch{}}
  if(executable)break;
 }
 if(!executable)throw Error('CLAUDE_DESKTOP_ENGINE_NOT_FOUND');
 const run=args=>{const value=cp.spawnSync(executable,args,{encoding:'utf8',stdio:['ignore','pipe','ignore'],timeout:20000,maxBuffer:65536});if(value.status!==0)throw Error('CLAUDE_PLUGIN_ADMIN_REFUSED');try{return JSON.parse(value.stdout);}catch{const last=value.stdout.trim().split('\n').pop();return JSON.parse(last);}};
 const id='agbrio-bridge@agbrio-observation';
 run(['plugin','marketplace','add',pack,'--json']);
 const list=run(['plugin','list','--json']);if(!Array.isArray(list))throw Error('CLAUDE_PLUGIN_ADMIN_INVALID_RESULT');
 run(['plugin',list.some(v=>v.id===id&&v.scope==='user')?'update':'install',id,'--scope','user','--json']);
 const installed=run(['plugin','list','--json']);if(!Array.isArray(installed)||installed.filter(v=>v.id===id&&v.scope==='user').length!==1)throw Error('CLAUDE_PLUGIN_USER_INSTALL_REQUIRED');
 if(!installed.find(v=>v.id===id&&v.scope==='user').enabled)run(['plugin','enable',id,'--scope','user','--json']);
 const checked=run(['plugin','list','--json']);if(!Array.isArray(checked)||checked.filter(v=>v.id===id&&v.scope==='user'&&v.enabled===true).length!==1)throw Error('CLAUDE_PLUGIN_USER_ENABLE_REQUIRED');
 console.log('Installed component verified. Return to Agbrio and use Enable Claude notices and phone replies to configure it. Only reload plugins when the original conversation is idle. No account login or model task was started.');
}catch(error){console.error(/^(CLAUDE_)[A-Z_]+$/.test(error.message)?error.message:'CLAUDE_MACOS_INSTALL_UNCONFIRMED');process.exitCode=1;}
AGBRIO_CLAUDE_INSTALL
