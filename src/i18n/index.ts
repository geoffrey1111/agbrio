import {useSyncExternalStore} from "react";
import en from "./en.json";
import traditional from "./zh-TW.json";

import {getLanguage,getLanguagePreference,subscribe,type Language} from "./locale";
export {getLanguage,getLanguagePreference,resolveLanguage,setLanguagePreference} from "./locale";
export type {Language,LanguagePreference} from "./locale";
const dictionaries:Record<Exclude<Language,"zh-CN">,Record<string,string>>={en,"zh-TW":traditional};
export function useLanguage(){return useSyncExternalStore(subscribe,getLanguage,getLanguage);}
export function useLanguagePreference(){useLanguage();return useSyncExternalStore(subscribe,getLanguagePreference,getLanguagePreference);}
// Translate only explicitly marked application copy. Conversation content,
// names, file bytes, identifiers and outgoing instructions never pass here.
const sourceByTranslation=new Map<string,string>();
for(const dictionary of Object.values(dictionaries))for(const[source,value]of Object.entries(dictionary))if(!sourceByTranslation.has(value))sourceByTranslation.set(value,source);
export function t(source:string|null|undefined,...values:unknown[]):string {
 if(source==null)return "";
 const key=sourceByTranslation.get(source)??source;
 const language=getLanguage();const format=language==="zh-CN"?key:dictionaries[language][key]??key;
 return format.replace(/\{(\d+)\}/g,(match,index)=>Number(index)<values.length?String(values[Number(index)]??""):match);
}
