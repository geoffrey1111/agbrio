#!/usr/bin/env node
// Agbrio status line for Claude Code. Claude Code passes session JSON on stdin;
// only the subscription rate-limit windows are kept, written to
// %LOCALAPPDATA%\AIWorkRouter\data\claude-usage.json for the Agbrio Host to read.
// Nothing else from the session (paths, model text, transcript) is stored.
import {mkdirSync,renameSync,writeFileSync} from 'node:fs';
import {join,isAbsolute} from 'node:path';

const base=process.platform==='win32'?process.env.LOCALAPPDATA:process.platform==='darwin'&&process.env.HOME?join(process.env.HOME,'Library','Application Support'):null;

let input='';
process.stdin.setEncoding('utf8');
process.stdin.on('data',chunk=>{if(input.length<256*1024)input+=chunk;});
process.stdin.on('end',()=>{
 let data={};try{data=JSON.parse(input);}catch{/* print a plain line below */}
 const pick=w=>w&&Number.isFinite(w.used_percentage)&&w.used_percentage>=0&&w.used_percentage<=100?{usedPercentage:w.used_percentage,resetsAt:Number.isFinite(w.resets_at)?Math.trunc(w.resets_at):null}:null;
 const fiveHour=pick(data.rate_limits?.five_hour),sevenDay=pick(data.rate_limits?.seven_day);
 if((fiveHour||sevenDay)&&base&&isAbsolute(base)){
  try{
   const dir=join(base,'AIWorkRouter','data'),file=join(dir,'claude-usage.json'),temp=file+'.tmp';
   mkdirSync(dir,{recursive:true});
   writeFileSync(temp,JSON.stringify({observedAt:Date.now(),...(fiveHour&&{fiveHour}),...(sevenDay&&{sevenDay})}));
   renameSync(temp,file);
  }catch{/* the status line must never break Claude Code */}
 }
 const part=(label,w)=>w?`${label} ${Math.round(w.usedPercentage)}%`:null;
 const model=typeof data.model?.display_name==='string'?data.model.display_name:'Claude';
 process.stdout.write([model,part('5h',fiveHour),part('7d',sevenDay)].filter(Boolean).join(' · '));
});
