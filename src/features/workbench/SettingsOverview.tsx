import {ArrowRight, Bell, Link2, MonitorSmartphone, Globe} from "lucide-react";
export function SettingsOverview({navigate,onDevices}:{navigate:(surface:"NOTIFICATIONS"|"RUNTIME")=>void;onDevices?:()=>void}){
 const rows=[{label:"通知",detail:"Windows · 手机 · 邮件",Icon:Bell,open:()=>navigate("NOTIFICATIONS")},{label:"设备登录",detail:"配对与已登录设备",Icon:MonitorSmartphone,open:onDevices??(()=>navigate("NOTIFICATIONS"))},{label:"连接",detail:"Codex · 手机网站",Icon:Link2,open:()=>navigate("RUNTIME")},{label:"ChatGPT",detail:"浏览器与登录",Icon:Globe,open:()=>navigate("RUNTIME")}];
 return <section className="v5-settings-page" aria-label="设置"><h1>设置</h1><div>{rows.map(({label,detail,Icon,open})=><button type="button" key={label} onClick={open}><Icon size={20}/><span><strong>{label}</strong><small>{detail}</small></span><ArrowRight size={18}/></button>)}</div><button className="v5-settings-advanced" type="button" onClick={()=>navigate("RUNTIME")}>运行环境</button></section>;
}
