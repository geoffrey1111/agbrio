import type {CodexWatch} from './CodexNotifications';
export type CreationInput={requestId:string;provider:string;directory:string;model:string|null;effort:string|null;text:string};
export type CreationModel={model:string;displayName?:string;supportedReasoningEfforts?:{reasoningEffort:string}[]};
export type CreationOptions={providers:{id:string;label:string;available:boolean;reason?:string;models?:CreationModel[]}[];projects:string[];models:CreationModel[]};
export type CreationReceipt={requestId:string;status:string;nativeId?:string|null;result?:{watch?:CodexWatch;delivery?:{status?:string};firstMessageSent?:boolean;error?:string}|null};
export interface ConversationCreationApi {options():Promise<CreationOptions>;create(input:CreationInput):Promise<CreationReceipt>;receipt(requestId:string):Promise<CreationReceipt>}
