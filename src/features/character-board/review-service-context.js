// Only the explicitly registered trusted review application receives this context.
(()=>{const c=__AGBRIO_REVIEW_CONFIG__,base=new URL(c.base),source=new URL(c.sourcePrefix,base.origin);
 const rewrite=value=>{try{const u=new URL(String(value),document.baseURI);if(u.origin===base.origin&&u.pathname.startsWith(c.mount))return String(value);if(u.origin===source.origin&&u.pathname.startsWith(source.pathname))return c.mount+u.pathname.slice(source.pathname.length)+u.search+u.hash;return String(value);}catch{return value;}};
 const fetch=window.fetch.bind(window);window.fetch=(input,init)=>fetch(input instanceof Request?new Request(rewrite(input.url),input):rewrite(input),init);
 for(const [proto,key]of [[HTMLImageElement.prototype,'src'],[HTMLScriptElement.prototype,'src'],[HTMLLinkElement.prototype,'href'],[HTMLMediaElement.prototype,'src'],[HTMLVideoElement.prototype,'poster'],[HTMLSourceElement.prototype,'src']]){const d=Object.getOwnPropertyDescriptor(proto,key);if(d?.set)Object.defineProperty(proto,key,{...d,set(v){d.set.call(this,rewrite(v));}});}
 // Normalize only transport attributes in inert markup, never candidate identity,
 // comments, fingerprints or ledger data. Dynamic gallery HTML uses innerHTML.
 const inner=Object.getOwnPropertyDescriptor(Element.prototype,'innerHTML');
 const markup=value=>{if(typeof value!=='string'||!inner?.set||!inner.get)return value;const box=document.createElement('template');inner.set.call(box,value);for(const node of box.content.querySelectorAll('[src],[href],[poster],[data-wall-src]'))for(const key of ['src','href','poster','data-wall-src']){const old=node.getAttribute(key);if(old!==null)nativeSet.call(node,key,rewrite(old));}return inner.get.call(box);};
 const nativeSet=Element.prototype.setAttribute;
 if(inner?.set)Object.defineProperty(Element.prototype,'innerHTML',{...inner,set(value){inner.set.call(this,markup(value));}});
 const insert=Element.prototype.insertAdjacentHTML;Element.prototype.insertAdjacentHTML=function(position,value){return insert.call(this,position,markup(value));};
 const set=Element.prototype.setAttribute;Element.prototype.setAttribute=function(k,v){return set.call(this,k,['src','href','poster','data-wall-src'].includes(k)?rewrite(v):v);};
 addEventListener('click',e=>{const a=e.target.closest?.('a[href]');if(!a)return;const raw=a.getAttribute('href');if(raw?.startsWith('#')){e.preventDefault();location.hash=raw;}else if(raw){const mapped=rewrite(raw);if(mapped!==raw)set.call(a,'href',mapped);}},true);
 // Existing backups stay in the original same-origin namespace; nothing is copied.
 if(c.legacyBase){const current='card-board:review-backup:v1:'+new URL('.',base).href+':',old='card-board:review-backup:v1:'+new URL('.',c.legacyBase).href+':';const toOld=k=>String(k).startsWith(current)?old+String(k).slice(current.length):k;const get=Storage.prototype.getItem,put=Storage.prototype.setItem,remove=Storage.prototype.removeItem,key=Storage.prototype.key;Storage.prototype.getItem=function(k){return get.call(this,toOld(k))};Storage.prototype.setItem=function(k,v){return put.call(this,toOld(k),v)};Storage.prototype.removeItem=function(k){return remove.call(this,toOld(k))};Storage.prototype.key=function(n){const k=key.call(this,n);return k?.startsWith(old)?current+k.slice(old.length):k;};}
})();
