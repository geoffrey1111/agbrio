import {it,expect} from 'vitest';import {readFileSync} from 'node:fs';
import {createRequire} from 'node:module';
const {JSDOM}=createRequire(import.meta.url)('jsdom');
const source=readFileSync('src/features/character-board/review-service-context.js','utf8');
it('keeps original review identity text and draft namespace while rewriting only transport attributes',()=>{
 const dom=new JSDOM('<!doctype html><base href="https://review.fixture/v1/mobile/review-boards/abc/frame/"><div id="gallery"></div>',{url:'https://review.fixture/v1/mobile/review-boards/abc/frame/',runScripts:'outside-only'});const w=dom.window;w.fetch=async(url:string)=>({url});
 const old='card-board:review-backup:v1:https://review.fixture/board/:pending';w.localStorage.setItem(old,'original unsent comment');
 w.eval(source.replace('__AGBRIO_REVIEW_CONFIG__',JSON.stringify({base:w.location.href,mount:'/v1/mobile/review-boards/abc/frame/',sourcePrefix:'/board/',legacyBase:'https://review.fixture/board/'})));
 const gallery=w.document.querySelector('#gallery');gallery.innerHTML='<img src="/board/assets/abc"><p>/board/comment stays</p><video src="/board/assets/video" poster="/board/assets/poster"></video>';
 expect(gallery.querySelector('img').getAttribute('src')).toBe('/v1/mobile/review-boards/abc/frame/assets/abc');expect(gallery.querySelector('p').textContent).toBe('/board/comment stays');expect(gallery.querySelector('video').getAttribute('poster')).toContain('/review-boards/abc/frame/assets/poster');
 const current='card-board:review-backup:v1:'+w.location.href+':pending';expect(w.localStorage.getItem(current)).toBe('original unsent comment');w.localStorage.setItem(current,'new comment');expect(w.localStorage.getItem(old)).toBe('new comment');expect(w.localStorage.length).toBe(1);
 const another='card-board:review-backup:v1:https://review.fixture/v1/mobile/review-boards/other/frame/:pending';expect(w.localStorage.getItem(another)).toBeNull();
 w.close();
});

it('root-mounted source URL rewriting is idempotent for already scoped URLs and hash navigation',async()=>{const mount='/v1/mobile/review-boards/abc/frame/';const dom=new JSDOM('<!doctype html><base href="https://review.fixture'+mount+'">',{url:'https://review.fixture'+mount,runScripts:'outside-only'});const w=dom.window;w.fetch=async(url:string)=>({url});w.eval(source.replace('__AGBRIO_REVIEW_CONFIG__',JSON.stringify({base:w.location.href,mount,sourcePrefix:'/',legacyBase:null})));const image=w.document.createElement('img');image.src=mount+'assets/a';expect(image.src).toBe('https://review.fixture'+mount+'assets/a');image.src='/assets/a';expect(image.src).toBe('https://review.fixture'+mount+'assets/a');image.setAttribute('src',image.src);expect(image.src).toBe('https://review.fixture'+mount+'assets/a');const a=w.document.createElement('a');a.setAttribute('href','#approvals');expect(a.getAttribute('href')).toBe('#approvals');w.close();});
