import {describe,expect,it} from "vitest";
import {assistantMcpConfig} from "./assistantClientInstructions";
import {assistantSetupInstructions,assistantTakeoverInstructions} from "./assistantSetupInstructions";
describe("client-independent connection instructions",()=>{
 it("exports only credential-free standard configuration",()=>{
  expect(JSON.parse(assistantMcpConfig("https://fixture.invalid/mcp")!)).toEqual({mcpServers:{agbrio:{type:"streamableHttp",url:"https://fixture.invalid/mcp"}}});
  for(const url of [null,"http://fixture.invalid/mcp","https://user:secret@fixture.invalid/mcp","https://fixture.invalid/mcp?token=private","https://fixture.invalid/mcp#private","https://fixture.invalid/other","not a url"]){expect(assistantMcpConfig(url)).toBeNull();}
 });
 it.each(["zh-CN","zh-TW","en"] as const)("keeps both general routes independent of Dot in %s",language=>{
  for(const route of ["workbuddy","generic"] as const){
   const prompt=assistantSetupInstructions("https://fixture.invalid/mcp",language,route);
   expect(prompt).toContain(route==="workbuddy"?'"type": "streamableHttp"':"Transport: Streamable HTTP");expect(prompt).toContain("OAuth");expect(prompt).toContain("PKCE");expect(prompt).toContain("Dot");
   expect(prompt).not.toContain("https://chatgpt.com/plugins");expect(prompt).not.toContain("https://learn.chatgpt.com/docs/dots");
   const takeover=assistantTakeoverInstructions(language,route);for(const guard of ["requestId","payloadHash","UNKNOWN","ruleId=null","decisionId=null","receipt","bindingRevision"]){expect(takeover).toContain(guard);}
   if(language==="en")expect(prompt).not.toMatch(/[\u3400-\u9fff]/);
  }
 });
 it("gates event wake and reports instead of claiming every MCP client can run unattended",()=>{
  const prompt=assistantSetupInstructions(null,"en","workbuddy");
  expect(prompt).toContain("ON-DEMAND ONLY");expect(prompt).toContain("do not create a polling task");expect(prompt).toContain("not installed tools");expect(prompt).toContain("no pasted bearer token");expect(prompt).toContain("refresh_token");expect(prompt).not.toContain('"mcpServers"');
  expect(assistantSetupInstructions("https://fixture.invalid/mcp","en","cloud")).toContain("ChatGPT Plugins");
  expect(assistantSetupInstructions("https://fixture.invalid/mcp","en","local")).toContain("local Codex");
 });
});

it.each(['zh-CN','zh-TW','en'] as const)('creation instructions expose live model, reasoning and speed choices in %s',language=>{const text=assistantTakeoverInstructions(language,'generic');for(const key of ['agbrio_list_codex_models','model','effort','serviceTier','default','settings'])expect(text).toContain(key);});
