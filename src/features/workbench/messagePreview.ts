/** Plain list summary only; the full original and send payload stay untouched. */
export function messagePreview(text:string){
 return text.slice(0,2000)
  .replace(/```[^\n]*\n?([\s\S]*?)(?:```|$)/g,'$1')
  .replace(/!?\[([^\]]+)\]\([^\n)]*\)/g,'$1')
  .replace(/^\s{0,3}(?:#{1,6}\s+|>\s*|[-*+]\s+)/gm,'')
  .replace(/\*\*([^*]+)\*\*|__([^_]+)__|`([^`]+)`/g,(_m,bold,under,code)=>bold??under??code)
  .replace(/\s+/g,' ').trim();
}
