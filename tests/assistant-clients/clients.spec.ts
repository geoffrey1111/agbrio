import {expect,test} from "@playwright/test";
test.beforeEach(async({page})=>{
 await page.addInitScript(()=>Object.defineProperty(navigator,"clipboard",{value:{writeText:async(text:string)=>{(window as unknown as {copied:string}).copied=text;}}}));
});
test("WorkBuddy copies only its setup/configuration and preserves ordinary cloud/local routes",async({page})=>{
 await page.goto("/tests/assistant-clients/");await page.getByRole("button",{name:"WorkBuddy",exact:true}).click();
 for(const button of await page.locator(".assistant-route-options button").all()){expect((await button.boundingBox())!.height).toBeGreaterThanOrEqual(44);}
 await page.getByRole("button",{name:"复制给 AI 助手",exact:true}).click();
 const prompt=await page.evaluate(()=>(window as unknown as {copied:string}).copied);
 expect(prompt).toContain("~/.workbuddy/mcp.json");expect(prompt).not.toContain("https://chatgpt.com/plugins");expect(prompt).toContain("UNKNOWN");
 await page.getByText("自己操作：查看详细教程",{exact:true}).click();await page.getByRole("button",{name:/添加 MCP 连接/}).click();
 await page.getByRole("button",{name:"复制 MCP 配置",exact:true}).click();
 const config=JSON.parse(await page.evaluate(()=>(window as unknown as {copied:string}).copied));expect(config.mcpServers.agbrio).toEqual({type:"streamableHttp",url:"https://instance.fixture.invalid/mcp"});
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 await page.getByRole("button",{name:/ChatGPT/}).click();await page.getByRole("button",{name:/注册云端插件/}).click();await expect(page.getByRole("button",{name:"在浏览器打开插件首页",exact:true})).toBeVisible();
 await page.getByRole("button",{name:/Codex/}).click();await page.getByRole("button",{name:/添加到 Codex/}).click();await expect(page.getByRole("button",{name:"在浏览器打开插件首页",exact:true})).toHaveCount(0);
});
test("generic onboarding stays within the phone layout and never claims unattended acceptance",async({page})=>{
 await page.goto("/tests/assistant-clients/?lang=en");await page.getByRole("button",{name:"Other MCP assistant",exact:true}).click();
 await expect(page.getByText(/Continuous handling and daily reports require separate/)).toBeVisible();
 await page.getByText("Do it yourself: detailed guide",{exact:true}).click();const nav=page.getByRole("navigation");
 for(let index=0;index<6;index++){await nav.getByRole("button").nth(index).click();expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);}
 await page.getByRole("button",{name:"Copy collaboration instructions",exact:true}).click();
 expect(await page.evaluate(()=>(window as unknown as {copied:string}).copied)).toContain("ON-DEMAND ONLY");
 await expect(page.getByText("Awaiting actual evidence",{exact:true})).toHaveCount(0);
});
