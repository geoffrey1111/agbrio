import bridge from './board-frame.js?raw';
import inputCss from '../../styles/mobile-inputs.css?raw';
/** srcdoc retains an opaque origin; CSP is before all source scripts. No channel or credential is persisted. */
export function boardDocument(html:string,attempt=0){
 const head=`<head><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; connect-src 'none'; frame-src 'none'; form-action 'none'; base-uri 'none'"><script>${bridge.replace(/<\/script/gi,'<\\/script')}</script><!-- board-attempt:${attempt} -->`;
 const sizedHead=`${head}<style id="agbrio-mobile-inputs">${inputCss}</style>`;
 return /<head\s*>/i.test(html)?html.replace(/<head\s*>/i,sizedHead):`${sizedHead}</head>${html}`;
}
