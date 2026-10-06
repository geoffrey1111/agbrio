import { useRef, useState } from "react";
/** Bridge is a presentation of one Workstream and its ACTIVE endpoints. */
type Endpoint = { label: string; externalId: string } | null | undefined;
export function securityRecoveryError(error: unknown, action: "OPEN" | "COMPLETE") {
  const detail = String(error);
  if (detail.includes("SECURITY_RECOVERY_BUSY")) return "验证窗口正在打开或交接，请稍候再试。自动操作保持暂停。";
  if (detail.includes("SECURITY_RECOVERY_NOT_OPENED")) return "请先点击『打开或显示 Router 浏览器』，确认已登录且没有验证提示后再继续。";
  if (detail.includes("SECURITY_BROWSER_FOCUS_BLOCKED")) return "Router 的 Chromium 已打开，但 Windows 阻止了切换到前台。请从任务栏切换到 Chromium；无需再开一个浏览器。";
  if (detail.includes("SECURITY_BROWSER_WINDOW_UNAVAILABLE")) return "Router 浏览器进程存在，但窗口尚不可用。自动操作保持暂停，请稍后重试显示窗口。";
  if (detail.includes("SECURITY_BROWSER_CLOSE_PENDING")) return "验证窗口尚未安全结束。请关闭该验证窗口后，再点击『已登录，继续』。自动操作保持暂停。";
  if (detail.includes("PROFILE_IN_USE")) return "验证浏览器仍被其他会话占用。Router 已保持自动操作暂停，请保留当前窗口。";
  return action === "OPEN" ? "Router 浏览器未能显示，自动操作保持暂停。请重试显示窗口。" : "验证窗口尚未安全结束，自动操作保持暂停。请重试『已登录，继续』。";
}
export function BridgeStatus({ name, decision, execution, authenticationRequired, executionConnected,
  nextAction, recoveryError, showErrors = false, recoveryOnly = false, onManage, onOpenBrowser, onAuthenticationCompleted }: {
  name: string; decision: Endpoint; execution: Endpoint; authenticationRequired?: boolean;
  executionConnected?: boolean; nextAction?: string | null; onManage: () => void;
  recoveryError?: string | null;
  showErrors?: boolean;
  recoveryOnly?: boolean;
  onOpenBrowser?: () => void | Promise<unknown>; onAuthenticationCompleted?: () => void | Promise<unknown>;
}) {
  const pending = useRef(false);
  const [securityBusy, setSecurityBusy] = useState<"OPEN" | "COMPLETE" | null>(null);
  const [securityWindowShown, setSecurityWindowShown] = useState(false);
  async function securityAction(action: "OPEN" | "COMPLETE") {
    if (pending.current) return;
    pending.current = true;
    setSecurityBusy(action);
    try {
      const result = await (action === "OPEN" ? onOpenBrowser?.() : onAuthenticationCompleted?.());
      if (action === "OPEN") setSecurityWindowShown(result !== false);
    }
    finally { pending.current = false; setSecurityBusy(null); }
  }
  return <section className={`v3-bridge-status${authenticationRequired ? " v3-bridge-recovery" : ""}`} aria-label={`${name} Bridge 两端状态`}>
    {!recoveryOnly && <div className="v3-bridge-bindings">
      <div><small>控制端 · ChatGPT · 决定下一步</small><strong>{decision?.label || "尚未绑定"}</strong>
        <span>{authenticationRequired ? "自动操作已暂停 · 等待你确认恢复" : decision ? "绑定已保存" : "选择一条现有对话"}</span></div>
      <div><small>执行端 · Codex · 完成任务</small><strong>{execution?.label || "尚未绑定"}</strong>
        <span>{execution ? executionConnected === false ? "绑定已保存 · 服务未连接" : "绑定已保存" : "选择一条现有对话"}</span></div>
    </div>}
    <div className="v3-bridge-next-action">{(authenticationRequired || nextAction || !decision || !execution) && <p role={authenticationRequired ? "status" : undefined}>{authenticationRequired ? "自动操作已暂停；这不代表当前未登录。在 Router 浏览器确认后继续。"
      : nextAction || "先选择两端对话，再开始工作。"}</p>}
      <div>{!recoveryOnly && <button type="button" onClick={onManage}>管理两端绑定</button>}
        {onOpenBrowser ? <button type="button" className={authenticationRequired ? "v3-primary" : undefined} disabled={securityBusy !== null} onClick={() => { void securityAction("OPEN"); }}>{securityBusy === "OPEN" ? "正在显示浏览器…" : authenticationRequired ? "打开或显示 Router 浏览器" : "打开 Router 浏览器"}</button> : null}
        {authenticationRequired && onAuthenticationCompleted ? <button type="button" disabled={securityBusy !== null} onClick={() => { void securityAction("COMPLETE"); }}>{securityBusy === "COMPLETE" ? "正在恢复自动操作…" : "已登录，继续"}</button> : null}</div>
    </div>
    {authenticationRequired && <><p className="v4-meta">继续时关闭验证窗口，保留登录并恢复自动操作。</p><details className="v4-secondary-details"><summary>已登录，为什么仍需要确认？</summary><p>之前的验证事件让自动操作暂停，不代表当前未登录。电脑上由 Router 打开的 Chromium 就是确认窗口。已登录且没有验证提示时，无需重新登录，直接点「已登录，继续」。继续时该窗口会关闭，Router 沿用同一登录恢复自动操作。</p></details></>}
    {(authenticationRequired || showErrors) && recoveryError ? <p role="alert">{recoveryError}</p> : null}
    {securityWindowShown && !recoveryError ? <p role="status">{authenticationRequired ? "已显示 Router 的 Chromium 窗口，沿用 Router 保存的登录。已登录且没有验证提示时，回到这里点击『已登录，继续』。" : "已显示 Router 的 Chromium 窗口，沿用现有登录。保存绑定后，这个窗口会保持打开。"}</p> : null}
  </section>;
}
