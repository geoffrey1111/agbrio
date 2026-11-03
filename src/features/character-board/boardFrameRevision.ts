const MAX_PROBE_BYTES=2*1024*1024;
export function bootstrapRevision(text:string):string|null{
 try{const value=JSON.parse(text);if(!value||typeof value.manifest!=='object'||!value.manifest||Array.isArray(value.manifest))return null;return JSON.stringify([value.manifest,value.scripts??{},value.counts??{}]);}catch{return null;}
}
/** Read only the bounded JSON bootstrap. Never execute or insert fetched HTML. */
export async function probeBoardRevision(src:string,signal:AbortSignal):Promise<string|null>{
 const response=await fetch(src,{credentials:'same-origin',cache:'no-store',signal});
 if(!response.ok||Number(response.headers.get('content-length'))>MAX_PROBE_BYTES)return null;
 if(!response.body)return null;
 const reader=response.body.getReader(),parts:Uint8Array[]=[];let size=0;
 try{while(true){const next=await reader.read();if(next.done)break;size+=next.value.byteLength;if(size>MAX_PROBE_BYTES)return null;parts.push(next.value);}}finally{await reader.cancel().catch(()=>{});}
 const bytes=new Uint8Array(size);let at=0;for(const part of parts){bytes.set(part,at);at+=part.length;}
 const html=new TextDecoder().decode(bytes);const match=/<script\b[^>]*\bid\s*=\s*(["'])board-bootstrap\1[^>]*>([\s\S]*?)<\/script\s*>/i.exec(html);
 return match?bootstrapRevision(match[2]):null;
}
