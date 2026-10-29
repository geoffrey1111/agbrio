export const WATCH_NOTIFICATION_MESSAGE="AGBRIO_OPEN_WATCH";
/** Navigation metadata only; never reply text, credentials or approval. */
export function watchNotificationRoute(input:string,origin:string){
 try{const url=new URL(input,origin);if(url.origin!==origin||url.pathname!=="/mobile/notifications")return null;
  const value=url.searchParams.get("event");if(!value||!/^[1-9]\d*$/.test(value))return null;
  const sequence=Number(value);return Number.isSafeInteger(sequence)?{sequence,url:url.href}:null;
 }catch{return null;}
}
