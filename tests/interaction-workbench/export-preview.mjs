import {build} from 'esbuild';
import {readFile,readdir,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';

const target=process.argv[2];
if(!target)throw new Error('Pass an output HTML path');
const dir=import.meta.dirname;
const result=await build({entryPoints:[resolve(dir,'fixture.tsx')],bundle:true,format:'iife',jsx:'automatic',minify:true,write:false,outdir:dir,loader:{'.png':'dataurl','.svg':'dataurl'},define:{'import.meta.env':'{}'},external:['/fonts/*'],plugins:[{name:'raw-assets',setup(builder){
 builder.onResolve({filter:/\?raw$/},args=>({path:resolve(args.resolveDir,args.path.slice(0,-4)),namespace:'raw'}));
 builder.onLoad({filter:/.*/,namespace:'raw'},async args=>({contents:await readFile(args.path,'utf8'),loader:'text'}));
}}]});
const css=result.outputFiles.find(f=>f.path.endsWith('.css')).text.replace(/@font-face\s*\{[^{}]*\/fonts\/[^{}]*\}/g,'');
const avatarDir=resolve(dir,'../../public/avatars');
const avatarFiles=await readdir(avatarDir);
const avatarData=Object.fromEntries(await Promise.all(avatarFiles.filter(f=>f.endsWith('.svg')).map(async file=>[file,'data:image/svg+xml;base64,'+(await readFile(resolve(avatarDir,file))).toString('base64')])));
const js=result.outputFiles.find(f=>f.path.endsWith('.js')).text.replace(/<\/script/gi,'<\\/script');
const source=await readFile(resolve(dir,'index.html'),'utf8');
const api=`<script>
const avatarData=${JSON.stringify(avatarData)};
const srcProperty=Object.getOwnPropertyDescriptor(HTMLImageElement.prototype,'src');
Object.defineProperty(HTMLImageElement.prototype,'src',{...srcProperty,set(src){
 const name=typeof src==='string'&&src.startsWith('/avatars/')?src.slice(9):null;
 srcProperty.set.call(this,(name&&avatarData[name])||src);
}});
const board={id:'demo-board',title:'演示交付看板',revision:'demo-1',available:true};
let boardVisible=true;
window.fetch=(input,init)=>{
 const route=typeof input==='string'?input:input.url;
 if(!route.includes('/v1/mobile/'))return Promise.reject(new Error('Offline preview'));
 const key=route.split('/v1/mobile/')[1].split('?')[0];
 const method=init?.method||'GET';
 const cache={bytes:0,uploadCount:0,eligibleBytes:0,eligibleCount:0,protectedBytes:0,protectedCount:0,removedCount:0,retentionDays:7};
 if(key==='boards/demo-board'&&method==='DELETE'){boardVisible=false;return Promise.resolve(new Response(null,{status:204}));}
 const data={'boards':boardVisible?[board]:[],'boards/demo-board':{board,html:'<main style="font:16px system-ui;padding:24px;color:#302820"><h2>演示交付看板</h2><p>本地离线样例 · 查看任务进度</p><ul><li>界面回归 · 进行中</li><li>设置流程 · 待验收</li></ul></main>',assets:[],inline:{}},'board-source':null,'app/capabilities':{boards:{cliPath:null}},'management-zones':{zones:[],candidates:[],legacyAssignments:[]},'cache':cache,'cache/cleanup':cache};
 const allowed=method==='GET'||(method==='POST'&&['boards','board-source','app/capabilities','management-zones','cache','cache/cleanup'].includes(key));
 return Promise.resolve(Response.json(allowed&&key in data?data[key]:{code:'PREVIEW_UNAVAILABLE'},{status:allowed&&key in data?200:404}));
};
</script>`;
await writeFile(resolve(target),source.replace('<html lang="zh-CN">','<html lang="zh-CN" data-offline-preview>').replace('<script type="module" src="./fixture.tsx"></script>',()=>`<style>${css}</style>${api}<script>${js}</script>`));
