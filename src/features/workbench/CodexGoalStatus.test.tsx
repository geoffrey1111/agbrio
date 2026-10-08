import {cleanup,render,screen} from "@testing-library/react";
import {afterEach,expect,it} from "vitest";
import {CodexGoalStatus,goalStatusText,goalElapsed} from "./CodexGoalStatus";
import {setLanguagePreference} from "../../i18n";
afterEach(()=>{cleanup();setLanguagePreference("zh-CN");});
it("preserves all native Goal statuses and never implies completion from blocking",()=>{
 const cases={active:"进行中的目标",paused:"已暂停的目标",blocked:"目标已停滞",budgetLimited:"目标受限",usageLimited:"目标使用受限",complete:"已达成目标"};for(const [status,label] of Object.entries(cases))expect(goalStatusText(status)).toBe(label);
 render(<CodexGoalStatus goal={{threadId:"demo",objective:"Review the completed reward display",status:"blocked",timeUsedSeconds:2814}}/>);expect(screen.getByText("目标已停滞")).toBeVisible();expect(screen.getByText("46m 54s")).toBeVisible();expect(screen.queryByText("已达成目标")).toBeNull();expect(goalElapsed(null)).toBeNull();
});
