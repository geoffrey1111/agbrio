import {createContext,useContext,useEffect,useState,type ReactNode} from 'react';
export type ComputerIdentity={hostName:string|null;platform?:string;features?:{pi?:boolean}};
const PiAvailability=createContext(true);
const HostPlatform=createContext<string|undefined>(undefined);
export const useHostPlatform=()=>useContext(HostPlatform);
export const usePiAvailable=()=>useContext(PiAvailability);
/** Presentation uses the connected Host, never the phone/browser OS. */
export function HostPiAvailability({identity,children}:{identity?:()=>Promise<ComputerIdentity>;children:ReactNode}){
 const [resolved,setResolved]=useState<{identity:typeof identity;available:boolean;platform?:string}>({identity:undefined,available:false});
 const available=!identity||(resolved.identity===identity&&resolved.available);
 useEffect(()=>{let alive=true;if(identity)void identity().then(value=>{if(alive)setResolved({identity,available:value.features?.pi!==false&&value.platform!=='macos',platform:value.platform});},()=>{if(alive)setResolved({identity,available:false});});return()=>{alive=false;};},[identity]);
 return <HostPlatform.Provider value={resolved.identity===identity?resolved.platform:undefined}><PiAvailability.Provider value={available}>{children}</PiAvailability.Provider></HostPlatform.Provider>;
}
