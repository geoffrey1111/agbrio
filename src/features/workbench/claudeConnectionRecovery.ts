import {t,type Language} from '../../i18n';
import manualInstall from './claudeManualInstall.ps1?raw';
export const claudeManualInstallPowerShell=manualInstall;
export function claudeRecoveryKind(error:unknown):'wait'|'host'|'update'|'configuration'|'manual'{
 const code=String(error).split(':')[0];
 if(/^CLAUDE_PLUGIN_ADMIN_REFUSED:(READ|SAVE)_CONFIGURATION:/.test(String(error)))return 'configuration';
 return code==='CLAUDE_CONNECTION_BUSY'?'wait':code==='CLAUDE_PLUGIN_DIFFERENT_HOST'?'host':code==='CLAUDE_DESKTOP_ENGINE_TOO_OLD'?'update':'manual';
}
const stages:Record<string,string>={LIST:'检查插件列表',ADD_MARKETPLACE:'添加内置插件来源',INSTALL:'安装插件',UPDATE:'更新插件',ENABLE:'启用插件',READ_CONFIGURATION:'读取连接配置',SAVE_CONFIGURATION:'保存连接配置',UNKNOWN:'执行插件命令'};
export function claudeFailureStage(error:unknown):string|null{
 const match=/^CLAUDE_PLUGIN_ADMIN_REFUSED:([A-Z_]+):(-?\d+|UNKNOWN)$/.exec(String(error));
 if(!match)return null;
 return `${t(stages[match[1]]??stages.UNKNOWN)} · ${t('退出码')} ${match[2]}`;
}
export function claudeCompletionInstructions(language:Language):string{
 if(language==='en')return `Check the Agbrio tools available in this Claude management conversation. Reuse working MCP authorization; do not add another server or sign in again without evidence.
Actually call agbrio_read_app read-only and report the granted scope, approval mode, expiry and one read result. Saved configuration or another client's success does not prove this session is connected.
If codex_wait is available, inspect status. Only for exact Codex turns already delegated by me, still needing follow-up, and covered by my request for completion notices: register the actual threadId/generation/turnId; promise a notice only after a WAITING receipt. Do not register every chat, create polling tasks, repeat a dispatch or send test messages.
If tools are missing, tell me the exact manual step. Only when this conversation is idle, ask me to run /reload-plugins; if that does not load the plugin, ask me to save work, fully quit and reopen Claude Desktop, then return to the same management conversation. Do not stop tasks or change permissions yourself.
Briefly report: what is actually connected, which tasks have confirmed notice registration, and what still needs my action.`;
 if(language==='zh-TW')return `請檢查目前這個 Claude 管理對話實際可用的 Agbrio 工具。已有可用 MCP 授權就直接復用，沒有證據不要重複新增或登入。
請實際唯讀呼叫 agbrio_read_app，回報授權範圍、審閱模式、有效期和一項讀取結果。不要僅憑已儲存設定或其他客戶端成功宣稱本對話已連接。
如果能發現 codex_wait，先查看 status。僅針對我已委託、仍需跟進且已要求完成通知的精確 Codex 輪次，使用實際 threadId/generation/turnId 登記；取得 WAITING 回執才承諾通知。不要登記所有對話、新增輪詢、重複派單或傳送測試訊息。
缺少工具時，明確說明需要我手動完成哪一步。僅在本對話空閒後，請我執行 /reload-plugins；仍未載入時，請我儲存工作並完整退出、重新開啟 Claude 桌面應用程式，再回到原管理對話。不要自行停止任務或修改權限。
請簡短回報：實際連接情況、已確認登記通知的任務，以及仍需我操作的步驟。`;
 return `请检查当前这个 Claude 管理对话实际可用的 Agbrio 工具。已有可用 MCP 授权就直接复用，没有证据不要重复添加或登录。
请实际只读调用 agbrio_read_app，回报授权范围、审批模式、有效期和一项读取结果。不要仅凭已保存配置或其他客户端成功宣称本对话已连接。
如果能发现 codex_wait，先查看 status。仅针对我已委托、仍需跟进且已要求完成通知的精确 Codex 轮次，使用实际 threadId/generation/turnId 登记；取得 WAITING 回执才承诺通知。不要登记所有对话、新增轮询、重复派单或发送测试消息。
缺少工具时，明确说明需要我手动完成哪一步。仅在本对话空闲后，请我执行 /reload-plugins；仍未加载时，请我保存工作并完整退出、重新打开 Claude 桌面应用，再回到原管理对话。不要自行停止任务或修改权限。
请简短回报：实际连接情况、已确认登记通知的任务，以及仍需我操作的步骤。`;
}
