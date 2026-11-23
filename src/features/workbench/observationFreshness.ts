import {useEffect,useState} from 'react';

/** Last verified state and last message are separate clocks. */
const OBSERVATION_MAX_AGE_MS=90_000;
export const observationExpired=(checkedAt:number,now:number)=>
 Number.isFinite(checkedAt)&&checkedAt>0&&now-checkedAt>OBSERVATION_MAX_AGE_MS;
export const claimsExecution=(state:string)=>state==='RUNNING'||state==='THINKING';

export function useObservationClock(){
 const [clock,setClock]=useState(Date.now);
 useEffect(()=>{
  const tick=()=>{if(!document.hidden)setClock(Date.now());};
  const timer=setInterval(tick,5000);
  document.addEventListener('visibilitychange',tick);
  window.addEventListener('focus',tick);
  return()=>{clearInterval(timer);document.removeEventListener('visibilitychange',tick);window.removeEventListener('focus',tick);};
 },[]);
 return clock;
}
