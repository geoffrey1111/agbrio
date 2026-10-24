// Server-owned trial eligibility, prepared-address capacity and UTC expiry.
// Machine identifiers arrive product-hashed; raw machine identifiers and IPs never persist.
export async function trialRequest(control,req,input,p,{validId,validKey,hash,hostToken,json,fail}){
 const s=control.ctx.storage,env=control.env;
 if(p.startsWith('/admin/trials/')){
  if(p==='/admin/trials/revoke'){
   if(!validId(input.tenantId))return fail('INVALID_TENANT');const t=await s.get('t:'+input.tenantId);
   if(!t||!['TRIAL','TRIAL_PENDING'].includes(t.kind))return fail('TRIAL_NOT_FOUND',404);
   const revoked={...t,revoked:true};await s.put('t:'+t.id,revoked);await env.TENANTS.get(env.TENANTS.idFromName(t.id)).configure(revoked);return json({revoked:true});
  }
  if(!Array.isArray(input.tenantIds)||!input.tenantIds.length||input.tenantIds.length>20||new Set(input.tenantIds).size!==input.tenantIds.length||input.tenantIds.some(id=>!validId(id)))return fail('INVALID_BATCH');
  for(const id of input.tenantIds){const allocated=await s.get('a:'+id);if(allocated&&allocated!=='TRIAL')return fail('ISSUANCE_CONFLICT',409);if(p.endsWith('/ready')&&!await s.get('p:'+id))return fail('TRIAL_NOT_PREPARED',409);}
  for(const id of input.tenantIds){const old=await s.get('p:'+id);await s.put({['a:'+id]:'TRIAL',['p:'+id]:old?{...old,ready:old.ready||p.endsWith('/ready')}:{id,ready:false,used:false}});}
  return json({prepared:input.tenantIds.length,ready:p.endsWith('/ready')});
 }
 if(!validKey(input.deviceKey)||!validKey(input.trialDeviceId))return fail('INVALID_DEVICE');
 const deviceHash=await hash(input.deviceKey),fingerprint=await hash('agbrio-trial:'+input.trialDeviceId),key='r:'+fingerprint;
 const record=await s.get(key),existingId=await s.get('d:'+deviceHash),existing=existingId?await s.get('t:'+existingId):null;
 if(existing&&(!existing.kind||existing.kind==='CODE'))return fail('TRIAL_NOT_ELIGIBLE',409);
 const reply=async t=>json({tenantId:t.id,origin:t.origin,expiresAt:t.expiresAt,kind:t.kind,serverNow:Date.now(),hostToken:await hostToken(env,t.id,input.deviceKey)});
 if(record){
  if(record.deviceHash!==deviceHash)return fail('TRIAL_ALREADY_USED',409);
  const t=await s.get('t:'+record.tenantId);if(!t||t.revoked)return fail('TRIAL_UNAVAILABLE',403);
  if(t.kind==='TRIAL'){await env.TENANTS.get(env.TENANTS.idFromName(t.id)).configure(t);return reply(t);} // Retry cannot extend an expired or active trial.
  if(t.expiresAt<=Date.now()){if(p==='/v1/trial/confirm')return fail('TRIAL_SETUP_EXPIRED',409);const pending={...t,expiresAt:Date.now()+30*60000};await s.put('t:'+t.id,pending);await env.TENANTS.get(env.TENANTS.idFromName(t.id)).configure(pending);return reply(pending);}
  if(p==='/v1/trial/confirm'){
   const tenant=env.TENANTS.get(env.TENANTS.idFromName(t.id));if(!await tenant.connected())return fail('TRIAL_HOST_NOT_READY',503);
   const startedAt=Date.now(),active={...t,kind:'TRIAL',startedAt,expiresAt:startedAt+7*86400000};
   await s.put({[key]:{...record,startedAt},['t:'+t.id]:active});await tenant.configure(active);return reply(active);
  }
  await env.TENANTS.get(env.TENANTS.idFromName(t.id)).configure(t);return reply(t);
 }
 if(existing)return fail('TRIAL_NOT_ELIGIBLE',409);
 if(p==='/v1/trial/confirm')return fail('TRIAL_NOT_PREPARED',409);
 const day=Math.floor(Date.now()/86400000),globalKey='q:'+day,ipKey='q:'+day+':'+await hash(env.ADMIN_TOKEN+':'+(req.headers.get('CF-Connecting-IP')??'unknown'));
 const count=await s.get(globalKey)??0,ipCount=await s.get(ipKey)??0;
 const limit=(value,fallback)=>Math.max(1,Math.min(100,Number(value)||fallback));
 if(count>=limit(env.TRIAL_DAILY_LIMIT,20)||ipCount>=limit(env.TRIAL_IP_DAILY_LIMIT,5))return fail('TRIAL_LIMIT_REACHED',429);
 const pool=await s.list({prefix:'p:'}),slot=[...pool.values()].find(t=>t.ready&&!t.used);
 if(!slot)return fail('TRIAL_CAPACITY_REACHED',503);
 const t={id:slot.id,origin:`https://ag-${slot.id}.${env.ZONE_ROOT}`,expiresAt:Date.now()+30*60000,kind:'TRIAL_PENDING',tokenHash:await hash(await hostToken(env,slot.id,input.deviceKey)),revoked:false};
 // Persist before RPC so retries recover the same address and setup deadline.
 await s.put({[key]:{tenantId:t.id,deviceHash,reservedAt:Date.now()},['d:'+deviceHash]:t.id,['t:'+t.id]:t,['p:'+t.id]:{...slot,used:true},[globalKey]:count+1,[ipKey]:ipCount+1});
 await env.TENANTS.get(env.TENANTS.idFromName(t.id)).configure(t);return reply(t);
}
