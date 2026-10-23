import bridge from './published-board-frame.js?raw';
import inputCss from '../../styles/mobile-inputs.css?raw';
import type {PublishedSnapshot} from './publishedBoardApi';
export function publishedBoardDocument(snapshot:PublishedSnapshot){
 const doc=new DOMParser().parseFromString(snapshot.html??'','text/html');
 doc.querySelectorAll('base,meta[http-equiv]').forEach(e=>e.remove());
 const local=(value:string)=>{try{const u=new URL(value,'https://board.invalid/');return u.origin==='https://board.invalid'?decodeURIComponent(u.pathname.slice(1)):null;}catch{return null;}};
 doc.querySelectorAll('script[src]').forEach(e=>{const key=local(e.getAttribute('src')??''),text=key?snapshot.inline[key]:undefined;if(text===undefined){e.remove();return;}e.removeAttribute('src');e.textContent=text.replace(/<\/script/gi,'<\\/script');});
 doc.querySelectorAll('link[rel="stylesheet"]').forEach(e=>{const key=local(e.getAttribute('href')??''),text=key?snapshot.inline[key]:undefined;const style=doc.createElement('style');style.textContent=(text??'').replace(/<\/style/gi,'<\\/style');e.replaceWith(style);});
 const csp=doc.createElement('meta');csp.httpEquiv='Content-Security-Policy';csp.content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; connect-src 'none'; font-src data:; frame-src 'none'; form-action 'none'; base-uri 'none'";
 const script=doc.createElement('script');script.textContent=bridge;doc.head.prepend(csp,script);
 const inputs=doc.createElement('style');inputs.id='agbrio-mobile-inputs';inputs.textContent=inputCss;doc.head.append(inputs);
 return '<!doctype html>'+doc.documentElement.outerHTML;
}
