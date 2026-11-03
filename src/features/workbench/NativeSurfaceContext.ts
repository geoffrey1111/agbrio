import {createContext} from "react";
/** Presentation grammar only; never changes endpoint or writer authority. */
export const NativeSurfaceContext=createContext(false);
/** Navigation only. Never configures a connection or changes credentials. */
export const ConnectionSettingsNavigation=createContext<(()=>void)|null>(null);
