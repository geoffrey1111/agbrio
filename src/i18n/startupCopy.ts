import {getLanguage} from "./locale";
// Keep the boot shell independent of React and the full dictionaries. The
// corresponding catalog entries are checked by the localization tests.
const english:Record<string,string>={"Agbrio 正在连接":"Agbrio connecting","正在连接…":"Connecting…","重试":"Retry","连接暂时不可用":"Connection unavailable"};
const traditional:Record<string,string>={"Agbrio 正在连接":"Agbrio 正在連線","正在连接…":"正在連線…","重试":"重試","连接暂时不可用":"連線暫時不可用"};
export function startupText(source:string){const language=getLanguage();return language==="en"?english[source]??source:language==="zh-TW"?traditional[source]??source:source;}
