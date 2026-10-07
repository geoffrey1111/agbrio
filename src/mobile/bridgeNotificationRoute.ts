export const BRIDGE_NOTIFICATION_MESSAGE="AGBRIO_OPEN_BRIDGE";
export function bridgeNotificationRoute(input:string,origin:string){
 try{const u=new URL(input,origin);if(u.origin!==origin||u.pathname!=="/mobile")return null;
  const workstream=u.searchParams.get("workstream")?.trim();if(!workstream||workstream.length>256)return null;
  return {workstreamId:workstream,replyId:u.searchParams.get("reply")?.trim()||null,handoffId:u.searchParams.get("handoff")?.trim()||null};
 }catch{return null;}
}
