export type ReviewRequest={method:string;route:string;body?:unknown;token?:string|null};
export type ReviewResponse={status:number;json:Record<string,unknown>};
export function validReviewRequest(value:unknown):value is ReviewRequest{
 if(!value||typeof value!=='object'||!Object.keys(value).every(k=>['method','route','body','token'].includes(k)))return false;const v=value as ReviewRequest;
 if(typeof v.method!=='string'||typeof v.route!=='string')return false;
 const allowed=v.method==='GET'&&v.route==='/api/reviews'||v.method==='POST'&&(v.route==='/api/draft'||v.route==='/api/submit'||/^\/api\/notify\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(v.route));
 if(!allowed||v.token!=null&&(typeof v.token!=='string'||v.token.length>256))return false;
 try{return new TextEncoder().encode(JSON.stringify(v)).length<=64*1024;}catch{return false;}
}
