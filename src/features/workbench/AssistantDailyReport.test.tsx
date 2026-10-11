import {cleanup,fireEvent,render,screen} from "@testing-library/react";
import {afterEach,expect,it,vi} from "vitest";
import {AssistantDailyReport} from "./AssistantDailyReport";
import {dailyReportInstructions} from "./dailyReportInstructions";
import {assistantSetupInstructions,assistantTakeoverInstructions} from "./assistantSetupInstructions";
import {setLanguagePreference} from "../../i18n";
afterEach(()=>{cleanup();setLanguagePreference("zh-CN");vi.restoreAllMocks();});
const bridges=[{id:"example-bridge",name:"Example Bridge",bindings:{bindingRevision:2}}];
it("does not schedule or silently include Bridges; copy needs explicit read-only acceptance and destination",async()=>{
 const writeText=vi.fn().mockResolvedValue(undefined);Object.defineProperty(navigator,"clipboard",{configurable:true,value:{writeText}});
 render(<AssistantDailyReport bridges={bridges}/>);expect(writeText).not.toHaveBeenCalled();fireEvent.click(screen.getByRole("button",{name:"配置或管理日报"}));
 const copy=screen.getByRole("button",{name:"复制日报请求"});expect(copy).toBeDisabled();expect(screen.getByLabelText("每天本地时间")).toHaveValue("22:00");
 fireEvent.click(screen.getByRole("checkbox",{name:/助手已回报/}));fireEvent.change(screen.getByLabelText("发送到哪里"),{target:{value:"this dot conversation"}});fireEvent.change(screen.getByLabelText("时区（IANA）"),{target:{value:"Asia/Shanghai"}});expect(copy).toBeDisabled();fireEvent.click(screen.getByRole("checkbox",{name:/Example Bridge/}));expect(copy).toBeEnabled();
 fireEvent.change(screen.getByLabelText("时区（IANA）"),{target:{value:"bad/zone"}});expect(copy).toBeDisabled();fireEvent.change(screen.getByLabelText("时区（IANA）"),{target:{value:"Asia/Shanghai"}});
 fireEvent.click(copy);expect(writeText).toHaveBeenCalledOnce();const prompt=writeText.mock.calls[0][0];expect(prompt).toContain('"timeZone": "Asia/Shanghai"');expect(prompt).toContain('"bindingRevision": 2');expect(prompt).toContain("更新原任务");expect(prompt).toContain("未启用");
 fireEvent.change(screen.getByLabelText("操作"),{target:{value:"PAUSE"}});fireEvent.click(copy);expect(writeText.mock.calls[1][0]).toContain('"action": "PAUSE"');
});
it.each(["zh-CN","zh-TW","en"] as const)("keeps supported/unsupported, existing-task and exact readonly accounting rules in %s",language=>{
 const request={action:"UPSERT" as const,time:"22:00",timezone:"Asia/Shanghai",destination:"this conversation",bridges:[{id:"example-bridge",bindingRevision:2}]};
 const prompt=dailyReportInstructions(request,language);for(const literal of ["agbrio_read_report","nextCursor","handoffId","requestId","SENT","sentAt","SEND_CHAT","UNKNOWN","APPROVED","THIS_GRANT","IANA","PAUSE","22"]){expect(prompt).toContain(literal);}
 const setup=assistantSetupInstructions("https://example.invalid/mcp",language);const takeover=assistantTakeoverInstructions(language);for(const p of [setup,takeover])expect(p).toContain("22:00");
 if(language==="en"){expect(prompt).toContain("NOT ENABLED");expect(prompt).toContain("PARTIALLY VERIFIED");expect(prompt).toContain("do not create a duplicate");expect(prompt).not.toMatch(/[\u3400-\u9fff]/);}
});
