import {t} from '../../i18n';
export const boardExamples=[
 {id:'progress',title:'项目进展',description:'不用翻聊天，看看最近完成了什么。',request:'把当前项目的进度、最近完成的内容和需要我决定的事项，整理成手机看板。'},
 {id:'compare',title:'方案对比',description:'把几个方案放在一起，在手机上选。',request:'把当前项目的几个方案、截图和差异整理成对比看板，先供我查看，不自动提交决定。'},
 {id:'report',title:'资料与报告',description:'把文件、图表和阶段结论整理到一起。',request:'把当前项目的文档摘要、测试结果和阶段结论整理成手机看板。'},
] as const;
export function boardInstructions(request:string,cliPath?:string|null){return `${t(request)}\n\n${t('请在当前项目新建独立的看板目录，使用 HTML、内联 CSS 和 JavaScript；图片可用目录内的相对路径。不要包含密钥、聊天记录或无关文件。')}\n${t('在该目录创建 agbrio-board.json，明确列出允许手机读取的文件：')}\n${JSON.stringify({schemaVersion:1,title:t('我的项目看板'),entry:'index.html',files:['index.html']},null,2)}\n${t('将用到的本地图片、样式、脚本或 JSON 逐项加入 files；不要使用外部 CDN、网络请求、表单提交或跨页面跳转。通过 agbrio_capabilities 的 boards 发现本机 CLI；没有 MCP 时，在正在运行的 Agbrio 安装目录找到 app-control/control.ps1。')}\npwsh -NoProfile -File '${(cliPath??'<Agbrio>/app-control/control.ps1').replaceAll("'","''")}' -Command PublishBoard -Folder '<absolute-board-folder>'\n${t('命令返回后核对看板 ID 和版本，并在 Agbrio 看板中确认可读。后续更新同一目录中的文件即可，入口和手机连接由 Agbrio 提供。发布只登记本机目录，电脑需要在线；不要启动新隧道、申请域名或发送任何真实消息。')}`;}
const escape=(s:string)=>s.replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('"','&quot;');
export function exampleDocument(id:string){
 const example=boardExamples.find(e=>e.id===id)??boardExamples[0];
 const title=t(example.title);
 const rows=id==='compare'?[[t('方案 A'),t('更简洁，适合快速查看')],[t('方案 B'),t('信息更完整，适合详细比较')]]:id==='report'?[[t('阶段结论'),t('把本次结果、图表和待核对的问题放在这里。')],[t('相关资料'),t('显示文档摘要和来源，便于继续阅读。')]]:[[t('最近完成'),t('新界面已接入，测试报告已整理。')],[t('接下来'),t('检查手机体验，收集反馈。')],[t('需要你决定'),t('选择下一阶段优先处理的事项。')]];
 return `<!doctype html><html><head><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><style>*{box-sizing:border-box}body{margin:0;padding:32px 24px;background:#fffcf8;color:#342d27;font:16px/1.65 system-ui,sans-serif}main{max-width:760px;margin:auto}small{color:#76675a}h1{font-size:28px;line-height:1.3;margin:12px 0 32px}section{padding:20px 0;border-top:1px solid #e3ddd5}h2{font-size:18px;margin:0 0 8px}p{margin:0}</style></head><body><main><small>${escape(t('示例内容'))}</small><h1>${escape(title)}</h1>${rows.map(([a,b])=>`<section><h2>${escape(a)}</h2><p>${escape(b)}</p></section>`).join('')}</main></body></html>`;
}
