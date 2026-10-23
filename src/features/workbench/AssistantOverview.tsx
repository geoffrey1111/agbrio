import {useId} from "react";
import {ArrowRight,Bot,MessagesSquare,Plug,ShieldCheck} from "lucide-react";
import {t,useLanguage} from "../../i18n";
import {CopyAction} from "./NativeReaderActions";

const examples=[
 {title:"汇总进度",description:"把多个执行对话的进展、阻塞和待决策事项，整理到一个管理对话里。",prompt:"先发现 Agbrio 的实际工具和权限，只读查看我指定的对话或 Bridge，汇总进度、阻塞和需要我决定的事项。先问我要跟进哪些任务，不发送任何消息，不创建订阅或定时任务。"},
 {title:"交接任务",description:"把规划对话的要求交给执行对话，再把结果带回来；具体方向以实际工具权限为准。",prompt:"我想用 Agbrio 协调规划和执行。先读取实际能力，问我要使用哪个 Bridge、谁规划、谁执行，以及哪些情况必须问我。确认精确对话和授权范围后再提出交接方案；不要仅凭这段示例发送消息。"},
 {title:"集中处理决策",description:"让助手整理要你拍板的问题，等你回答后，再按授权继续跟进。",prompt:"先只读整理我指定任务里需要本人决定的问题，附上来源和可选方案。收到我的实际回答后，再按现有权限提出下一步；不要替我选择、提交原生决策或自动扩大接管范围。"},
];

export function AssistantOverview(){
 useLanguage();const id=useId();
 return <section className="assistant-overview" aria-labelledby={`${id}-title`}>
  <h2 id={`${id}-title`}>{t("让助手帮你跟进电脑上的任务")}</h2>
  <p>{t("把分散的进度、交接和待决策事项带回一个管理对话，少来回切换。")}</p>
  <details className="assistant-explainer"><summary>{t("AI 助手和 MCP 是怎么配合的？")}</summary>
   <div className="assistant-system-map" role="img" aria-label={t("你向助手说明任务，助手通过 MCP 调用 Agbrio，再读取或按权限操作电脑上的对话。") }>
    <span><Bot aria-hidden size={23}/><strong>{t("AI 助手")}</strong><small>{t("理解任务")}</small></span><ArrowRight aria-hidden/>
    <span><Plug aria-hidden size={23}/><strong>MCP</strong><small>{t("连接工具")}</small></span><ArrowRight aria-hidden/>
    <span><MessagesSquare aria-hidden size={23}/><strong>Agbrio</strong><small>{t("对接电脑上的对话")}</small></span>
   </div>
   <dl className="assistant-concepts"><div><dt>{t("助手负责理解")}</dt><dd>{t("你在自己的 AI 对话里说明目标，由助手阅读结果、整理信息和提出下一步。")}</dd></div><div><dt>{t("MCP 负责连接")}</dt><dd>{t("MCP 是让助手发现和调用工具的通用接口。连上 Agbrio 后，助手可用工具读取进度、准备交接等，无需模拟点击界面。")}</dd></div><div><dt>{t("Agbrio 负责执行与记录")}</dt><dd>{t("Agbrio 把操作交给精确绑定的对话，检查权限并保留回执；可执行哪些操作，以实际发现的能力为准。")}</dd></div></dl>
   <p className="assistant-owner-note"><ShieldCheck aria-hidden size={19}/>{t("你决定授权与接管范围；需要本人决定的问题，仍由你回答。")}</p>
   <p className="assistant-overview-note">{t("接入 MCP 不会自动启动后台助手。持续提醒、自动跟进和定时汇报，还需客户端支持并单独开启。电脑需保持开机、联网，Agbrio 保持运行。")}</p>
   <a href="https://modelcontextprotocol.io/docs/learn/architecture" target="_blank" rel="noreferrer">{t("了解 MCP 的官方说明")}<ArrowRight aria-hidden size={14}/></a>
  </details>
  <details className="assistant-examples"><summary>{t("可以让助手做什么？")}</summary><p className="assistant-overview-note">{t("下面是使用示例。复制后发给已连接的助手，再指定自己的任务；复制本身不会执行操作。")}</p><ul>{examples.map(example=><li key={example.title}><div><strong>{t(example.title)}</strong><p>{t(example.description)}</p></div><CopyAction text={t(example.prompt)} label={t("复制{0}示例",t(example.title))}/></li>)}</ul></details>
 </section>;
}
