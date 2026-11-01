// Snapshot displayed notices before committing a read. Never close a later push.
export async function readAndClearPush<T>(commit:()=>Promise<T>,matches:(data:Record<string,unknown>,target:URL)=>boolean|Promise<boolean>):Promise<T>{
 let notices:Notification[]=[];
 try{const r=await navigator.serviceWorker?.getRegistration();notices=await r?.getNotifications?.()??[];}catch{/* capability may be absent on a device */}
 const result=await commit();
 void (async()=>{for(const n of notices.slice(0,128)){try{const d=n.data??{},u=new URL(d.target??'',location.origin);if(u.origin===location.origin&&['/mobile','/mobile/notifications'].includes(u.pathname)&&await matches(d,u))n.close();}catch{/* unrelated/unresolvable/system-dismissed notice */}}})();
 return result;
}
