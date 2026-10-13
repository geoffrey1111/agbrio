// Validates the production early guard against an already-open ordinary Desktop.
// Every mutating dependency is replaced by a tripwire, even if Desktop closes
// during this check. No pin/copy, start/stop, model or installer action can run.
import assert from 'node:assert/strict';
import {resolve,join} from 'node:path';
import {randomUUID} from 'node:crypto';
import {lifecycle} from '../src-tauri/resources/codex-shared/control.mjs';
import {launchEntry} from '../src-tauri/resources/codex-shared/launch-entry.mjs';
assert.equal(process.platform,'win32');
const source=resolve('src-tauri/resources/codex-shared');
const directory=join(process.env.LOCALAPPDATA,'AIWorkRouter/codex-shared');
let writes=0;
const forbidden=()=>{writes++;throw Error('READONLY_SMOKE_MUTATION_REJECTED');};
await assert.rejects(launchEntry({source,directory,node:process.execPath},{
 control:options=>options.action==='status'?lifecycle(options):forbidden(),
 discover:forbidden,stage:forbidden
}),error=>error.message==='SHARED_DESKTOP_CLOSE_REQUIRED');
assert.equal(writes,0);
// A nonexistent owned fixture path supplies cold state without any disk write.
// Exercise the same preflight called by the app before desktop-runtime/Pin.
const cold=resolve('runtime/shared-native-compatibility','readonly-'+randomUUID());
for(const action of ['prepare-check','setup'])await assert.rejects(lifecycle({directory:cold,scripts:source,action},{atomic:forbidden,spawn:forbidden,run:forbidden,verifyRuntime:forbidden}),error=>error.message==='SHARED_DESKTOP_CLOSE_REQUIRED');
assert.equal(writes,0);
console.log(JSON.stringify({ordinaryDesktopGuard:'PASS',appPreparationGuard:'PASS',directSetupGuard:'PASS',mutatingCalls:0,nativeCopies:0,desktopStarts:0,processStops:0,modelRequests:0}));
