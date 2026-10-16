import {defineConfig} from "@playwright/test";
export default defineConfig({
 testDir:"tests/assistant-clients",workers:1,timeout:30000,
 outputDir:"runtime/workbuddy-20261017/browser-output",reporter:"list",
 use:{baseURL:"http://127.0.0.1:18429",browserName:"chromium",locale:"zh-CN",screenshot:"only-on-failure"},
 projects:[{name:"phone",use:{viewport:{width:390,height:844},isMobile:true,hasTouch:true}},{name:"desktop",use:{viewport:{width:1280,height:900}}}],
 webServer:{command:"npm run dev -- --host 127.0.0.1 --port 18429",url:"http://127.0.0.1:18429",reuseExistingServer:false,timeout:30000},
});
