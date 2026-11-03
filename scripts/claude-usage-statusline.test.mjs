import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync,readFileSync,rmSync,existsSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {spawnSync} from 'node:child_process';

test('status line stores only bounded quota metadata in this platform data root',t=>{
 const root=mkdtempSync(join(tmpdir(),'agbrio-quota-'));t.after(()=>rmSync(root,{recursive:true}));
 const result=spawnSync(process.execPath,['scripts/claude-usage-statusline.mjs'],{encoding:'utf8',input:JSON.stringify({model:{display_name:'Fixture'},transcript:'PRIVATE_SOURCE',cwd:'PRIVATE_PATH',rate_limits:{five_hour:{used_percentage:12,resets_at:1234567890}}}),env:{...process.env,HOME:root,LOCALAPPDATA:root},timeout:5000});
 assert.equal(result.status,0);assert.match(result.stdout,/5h 12%/);
 const base=process.platform==='win32'?root:join(root,'Library','Application Support');
 if(!['win32','darwin'].includes(process.platform)){assert.equal(existsSync(join(root,'AIWorkRouter')),false);return;}
 const value=JSON.parse(readFileSync(join(base,'AIWorkRouter','data','claude-usage.json'),'utf8'));
 assert.deepEqual(Object.keys(value).sort(),['fiveHour','observedAt']);assert.equal(value.fiveHour.usedPercentage,12);assert.equal(JSON.stringify(value).includes('PRIVATE_'),false);
});
