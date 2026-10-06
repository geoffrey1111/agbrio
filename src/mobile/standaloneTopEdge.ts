/** Experimental WebKit standalone scroll-edge workaround. Not a layout inset.
 * Real-device report: https://qiita.com/na-trium-144/items/0add98a80ca2391e3f17
 * Empty text clipping preserves the edge color without painting over controls.
 * No full-screen overlay, animation or input handler.
 */
export function installStandaloneTopEdge(){
 const ios=/iPhone|iPad|iPod/.test(navigator.userAgent)||
  (navigator.platform==="MacIntel"&&navigator.maxTouchPoints>1);
 const standalone=Boolean((navigator as Navigator&{standalone?:boolean}).standalone)||
  Boolean(window.matchMedia?.("(display-mode: standalone),(display-mode: fullscreen)").matches);
 if(!ios||!standalone||!globalThis.CSS?.supports("background-clip","text"))return;
 if(document.getElementById("aiwr-standalone-top-edge"))return;
 const edge=document.createElement("div");
 edge.id="aiwr-standalone-top-edge";
 edge.setAttribute("aria-hidden","true");
 document.body.append(edge);
}
