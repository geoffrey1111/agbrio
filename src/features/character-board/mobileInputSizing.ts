import css from '../../styles/mobile-inputs.css?raw';

/** Same-origin board presentation only. Never pierce a foreign board's origin. */
export function sizeBoardInputs(frame:HTMLIFrameElement){
 try{
  const doc=frame.contentDocument;
  if(!doc?.head||doc.getElementById('agbrio-mobile-inputs'))return;
  const style=doc.createElement('style');style.id='agbrio-mobile-inputs';style.textContent=css;doc.head.append(style);
 }catch{/* Cross-origin boards keep their own presentation and security boundary. */}
}
