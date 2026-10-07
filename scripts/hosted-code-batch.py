"""Owner-only issuance tool; never bundled with desktop or given to recipients.

Generate locally first, then issue hashes after HTTPS/domain provisioning. Keep
operator.json and exported codes in ignored private storage. No codes printed.
"""
import argparse,hashlib,json,secrets,urllib.request
from pathlib import Path

def post(op,path,body):
 req=urllib.request.Request(op['controlOrigin']+path,data=json.dumps(body).encode(),headers={'Content-Type':'application/json','Authorization':'Bearer '+op['adminToken']})
 with urllib.request.urlopen(req,timeout=25) as response:return json.load(response)

def main():
 p=argparse.ArgumentParser();p.add_argument('action',choices=['generate','issue','ready','status','revoke','expire']);p.add_argument('--operator',type=Path,required=True);p.add_argument('--batch',type=Path);p.add_argument('--count',type=int,default=5);p.add_argument('--hash');a=p.parse_args();op=json.loads(a.operator.read_text(encoding='utf-8-sig'))
 if a.action=='generate':
  if a.batch is None or a.batch.exists() or not 1<=a.count<=20:raise ValueError('New private batch path and count 1..20 required')
  codes=[]
  for days in [7,30,365]:
   for _ in range(a.count):
    code=f'AGB{days}-'+secrets.token_hex(32);codes.append({'days':days,'code':code,'hash':hashlib.sha256(code.encode()).hexdigest(),'tenantId':secrets.token_hex(16)})
  a.batch.parent.mkdir(parents=True,exist_ok=True);a.batch.write_text(json.dumps(codes,indent=2),encoding='utf-8');print(json.dumps({'generated':len(codes),'issued':False}));return
 if a.action=='status':
  r=post(op,'/admin/status',{});print(json.dumps({'total':len(r['codes']),'redeemed':sum(bool(c['redeemedAt']) for c in r['codes']),'revoked':sum(c['revoked'] for c in r['codes'])}));return
 if a.action in ['revoke','expire']:
  if not a.hash or len(a.hash)!=64:raise ValueError('Exact code hash required')
  print(json.dumps(post(op,'/admin/'+a.action,{'hash':a.hash})));return
 codes=json.loads(a.batch.read_text(encoding='utf-8'))
 if a.action=='issue':
  post(op,'/admin/issue',{'codes':[{k:c[k] for k in ['hash','days','tenantId']} for c in codes]});print(json.dumps({'issued':len(codes),'ready':False}));return
 # Prove every distinct HTTPS hostname reaches this Worker before making codes usable.
 for c in codes:
  origin=f'https://ag-{c["tenantId"]}.{op["zoneRoot"]}'
  with urllib.request.urlopen(origin+'/__agbrio/entrance',timeout=20) as r:
   if json.load(r).get('service')!='Agbrio hosted relay':raise ValueError('Wrong entrance')
 post(op,'/admin/ready',{'hashes':[c['hash'] for c in codes]});print(json.dumps({'ready':len(codes)}))
if __name__=='__main__':main()
