import {expect,it} from 'vitest';
import {assistantProcessedVisible,type BridgeActivity} from './bridgeActivity';
const row:BridgeActivity={workstreamId:'fixture',bindingRevision:2,sides:[],assistantProcessed:{handoffId:'sent',observationId:'exact-source',sourceRole:'DECISION',destinationEndpointId:'target',destinationTurnId:'accepted-turn',sentAt:1000,expiresAt:301000}};
it('keeps assistant attribution through sync lag, expires, and never hides a pending decision',()=>{
 expect(assistantProcessedVisible(row,2000,0)).toBe(true);expect(assistantProcessedVisible(row,301000,0)).toBe(false);
 const side={role:'EXECUTION' as const,endpointId:'target',state:'RUNNING',checkedAt:2000,turnId:'accepted-turn'};
 expect(assistantProcessedVisible({...row,sides:[side]},2000,0)).toBe(false);
 expect(assistantProcessedVisible({...row,sides:[{...side,turnId:'old-turn',checkedAt:900}]},2000,0)).toBe(true);
 expect(assistantProcessedVisible({...row,sides:[side]},2000,20000)).toBe(true);
 expect(assistantProcessedVisible({...row,sides:[{...side,state:'ACTION_REQUIRED'}]},2000,0)).toBe(false);
});
