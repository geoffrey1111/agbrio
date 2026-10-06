export type Language = "zh-CN" | "zh-TW" | "en";
export type LanguagePreference = Language | "system";
const storageKey = "agbrio.language.v1";
const listeners = new Set<() => void>();
function stored():LanguagePreference {
 try {const value=localStorage.getItem(storageKey);return value==="en"||value==="zh-CN"||value==="zh-TW"?value:"system";} catch {return "system";}
}
let preference=stored();
export function resolveLanguage(languages:readonly string[]):Language {
 const primary=languages[0]?.toLowerCase()??"en";
 if(primary.startsWith("zh"))return /(?:tw|hk|mo|hant)/.test(primary)?"zh-TW":"zh-CN";
 return "en";
}
export function getLanguage():Language {return preference==="system"?resolveLanguage(navigator.languages?.length?navigator.languages:[navigator.language]):preference;}
export function getLanguagePreference():LanguagePreference {return preference;}
function announce(){document.documentElement.lang=getLanguage();listeners.forEach(listener=>listener());}
export function setLanguagePreference(next:LanguagePreference){
 if(!["system","en","zh-CN","zh-TW"].includes(next))return;
 preference=next;try{localStorage.setItem(storageKey,next);}catch{/* A blocked store must not block switching. */}
 announce();
}
export function subscribe(listener:()=>void){listeners.add(listener);return()=>{listeners.delete(listener);};}
if(typeof window!=="undefined"){
 window.addEventListener("languagechange",()=>{if(preference==="system")announce();});
 window.addEventListener("storage",event=>{if(event.key===storageKey||event.key===null){preference=stored();announce();}});
 document.documentElement.lang=getLanguage();
}
