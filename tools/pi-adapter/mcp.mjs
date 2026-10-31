// Native Pi MCP implementation with an exact owner-selected global config subset.
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';import {pathToFileURL} from 'node:url';
const file=process.env.AGBRIO_PI_ACCESS_FILE;const bytes=fs.readFileSync(file);if(crypto.createHash('sha256').update(bytes).digest('hex')!==process.env.AGBRIO_PI_ACCESS_DIGEST)throw Error('PI_ACCESS_CHANGED');
const resources=JSON.parse(bytes.toString('utf8')).resources??{};
function verify(){for(const f of resources.integrationSources??[]){if(crypto.createHash('sha256').update(fs.readFileSync(f.path)).digest('hex')!==f.sha256)throw Error('PI_INTEGRATION_SOURCE_CHANGED');}}
verify();const {createMcpExtension}=await import(pathToFileURL(resources.mcpModule.path).href);const {loadMcpConfig}=await import(pathToFileURL(resources.mcpConfigModule.path).href);
export default createMcpExtension({loadConfig:ctx=>{verify();const all=loadMcpConfig({agentDir:path.dirname(resources.mcpConfig.path),cwd:ctx.cwd,projectTrusted:false});if(all.errors.length)throw Error('PI_MCP_CONFIG_INVALID');const servers=all.servers.filter(s=>resources.mcpServers.includes(s.name)).map(s=>({...s,config:{...s.config,exposure:'direct'}}));if(servers.length!==resources.mcpServers.length)throw Error('PI_MCP_APPROVAL_CHANGED');return{servers,errors:[],autoEnableCodemode:false};}});
