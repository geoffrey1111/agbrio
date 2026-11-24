// Fixed, read-only adapter for an owner-registered review service. No shell or
// client-supplied module/command. Rust launches this with Node filesystem writes,
// native addons, workers and subprocesses denied by the permission model.
import {pathToFileURL} from 'node:url';
import path from 'node:path';
const {root,group,since,until}=JSON.parse(process.argv[1]);
try {
  const {status}=await import(pathToFileURL(path.join(root,'board.mjs')).href);
  const raw=await status({group:group||'',since});
  if(!Array.isArray(raw)||raw.length>10000)throw Error('limit');
  const rows=raw.filter(r=>!until||(r.time&&Date.parse(r.time)<Date.parse(until))).map(r=>{
    const row={group:r.group,groupId:r.postId||'',item:r.item,status:r.status,comment:r.comment,time:r.time,
      key:r.key,fingerprint:r.fingerprint,active:r.active===true};
    if(!['采用','退回','未审'].includes(row.status)||Object.entries(row).some(([k,v])=>k!=='active'&&(typeof v!=='string'||v.length>4096)))throw Error('invalid');
    return row;
  });
  const text=JSON.stringify(rows);
  if(Buffer.byteLength(text)>4*1024*1024)throw Error('limit');
  process.stdout.write(text);
} catch {
  // Do not return source exceptions, local paths, snippets or credentials.
  process.stderr.write('REVIEW_BOARD_STATUS_SOURCE_UNAVAILABLE');
  process.exitCode=1;
}
