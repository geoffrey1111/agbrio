import {t as uiText,useLanguage} from "../../i18n";
/** Bridge is a presentation of one Workstream and its ACTIVE endpoints. */
type Endpoint = { label: string; externalId: string } | null | undefined;
export function BridgeStatus({ name, decision, execution, executionConnected, nextAction, recoveryError, showErrors = false, onManage }: {
  name: string; decision: Endpoint; execution: Endpoint;
  executionConnected?: boolean; nextAction?: string | null; onManage: () => void;
  recoveryError?: string | null;
  showErrors?: boolean;
}) {
 useLanguage();
  return <section className="v3-bridge-status" aria-label={uiText("{0} Bridge 两端状态", name)}>
    <div className="v3-bridge-bindings">
      <div><small>{uiText("控制端 · 决定下一步")}</small><strong>{decision?.label || uiText("尚未绑定")}</strong>
        <span>{decision ? uiText("绑定已保存") : uiText("选择一条现有对话")}</span></div>
      <div><small>{uiText("执行端 · 完成任务")}</small><strong>{execution?.label || uiText("尚未绑定")}</strong>
        <span>{execution ? executionConnected === false ? uiText("绑定已保存 · 服务未连接") : uiText("绑定已保存") : uiText("选择一条现有对话")}</span></div>
    </div>
    <div className="v3-bridge-next-action">{(nextAction || !decision || !execution) && <p>{nextAction || uiText("先选择两端对话，再开始工作。")}</p>}
      <div><button type="button" onClick={onManage}>{uiText("管理两端绑定")}</button></div>
    </div>
    {showErrors && recoveryError ? <p role="alert">{uiText(recoveryError)}</p> : null}
  </section>;
}
