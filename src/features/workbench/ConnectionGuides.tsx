import {useId,useState} from 'react';
import {ArrowRight,Check,ChevronDown,Monitor,Smartphone,Share,EllipsisVertical,Plus,House,Cloud,MessageSquare} from 'lucide-react';
import {t,useLanguage} from '../../i18n';
import './connectionGuides.css';
import {BrandMark} from './BrandMark';

export function ConnectionGuide({kind,phase=1,open=false}:{kind:'shared'|'phone';phase?:number;open?:boolean}){
 useLanguage();const shared=kind==='shared';
 const steps=shared?[
  ['准备共享连接','点击“准备共享连接”，等待准备完成。'],
  ['从这里打开 Codex','当前任务结束后，自行退出原来的 Codex 窗口，再点击“打开共享 Codex”。'],
  ['选一个对话开始跟进','看到“Desktop 已接入共享连接”后，回到对话列表，点 + 监听电脑上的对话。'],
 ]:[
  ['开通手机入口','有兑换码：在这里开通托管连接，自动获得手机网址。已有自己的 HTTPS 入口：选择“自行配置”。'],
  ['用手机打开网址','在手机的 Safari 或 Chrome 中打开这里提供的网址。电脑需保持开机、联网，Agbrio 在托盘运行即可。'],
  ['配对自己的手机','在电脑上生成配对码，再填到手机网页。兑换码用于开通入口，配对码用于登录设备，是两种不同的码。'],
 ];
 return <details className="connection-guide" open={open}><summary>{t(shared?'第一次使用共享连接？':'第一次连接手机？')}<ChevronDown size={16}/></summary><div className="connection-guide-content">
  <p>{t(shared?'让 Codex 窗口和 Agbrio 接入同一组对话，手机才能查看进度并继续跟进。':'电脑上的 Agbrio 提供对话和文件；手机通过自己的网址访问。内容仍由这台电脑提供。')}</p>
  <div className="connection-map" aria-label={t('连接示意')}>
   <span><Monitor/><strong>{shared?'Codex':t('你的电脑')}</strong></span><ArrowRight aria-hidden/><span>{shared?<MessageSquare/>:<Cloud/>}<strong>{t(shared?'共享连接':'手机网址')}</strong></span><ArrowRight aria-hidden/><span>{shared?<BrandMark size={25}/>:<Smartphone/>}<strong>{shared?'Agbrio':t('你的手机')}</strong></span>
  </div>
  <ol className="connection-steps">{steps.map(([title,body],i)=><li key={title} aria-current={i+1===phase?'step':undefined} data-done={i+1<phase}><span className="connection-step-number" aria-hidden>{i+1<phase?<Check size={15}/>:i+1}</span><div><strong>{t(title)}</strong><p>{t(body)}</p></div></li>)}</ol>
  {shared&&<p>{t('手机网址与配对在“设置 → 设备”中完成。完成共享连接后，还需在那里连接自己的手机。')}</p>}
  <details className="connection-help"><summary>{t('卡在这一步？')}</summary><p>{t(shared?'显示“对话正在执行”：先等任务结束，不要强行关闭。显示“已准备好”还不等于窗口已接入，需要再从这里打开共享 Codex。以后使用共享模式，也从“打开共享 Codex”或开始菜单“Codex (Agbrio shared)”进入。':'显示“连接中”：先保持电脑端打开，再检查连接。手机打不开时，核对电脑显示的“网页已连接”和完整网址；不要把兑换码当配对码，也不用反复创建新连接。')}</p></details>
 </div></details>;
}

export function PwaInstallGuide({expanded=false}:{expanded?:boolean}){
 useLanguage();const[platform,setPlatform]=useState<'ios'|'android'>('ios');const prefix=useId();
 const steps=platform==='ios'?[
  {Icon:Share,title:'打开分享菜单',body:'在 Safari 中打开你的 Agbrio 网址，点“分享”。有些布局需要先点“更多”才能看到分享。'},
  {Icon:Plus,title:'添加到主屏幕',body:'在分享菜单向下找“添加到主屏幕”。如果显示“作为 Web App 打开”，保持开启，再点“添加”。'},
  {Icon:House,title:'从新图标打开',body:'回到主屏幕，点 Agbrio 图标打开。若提示登录，用电脑生成的新配对码登录。'},
 ]:[
  {Icon:EllipsisVertical,title:'打开浏览器菜单',body:'在 Chrome 中打开你的 Agbrio 网址，点右上角 ⋮ 菜单。'},
  {Icon:Plus,title:'安装或添加',body:'选择“添加到主屏幕”，再选择“安装”。部分版本直接显示“安装应用”；按浏览器提示确认即可。'},
  {Icon:House,title:'从新图标打开',body:'在主屏幕或应用列表找到 Agbrio，点击打开。若提示登录，用电脑生成的新配对码登录。'},
 ];
 return <details className="connection-guide pwa-install-guide" open={expanded}><summary><Smartphone size={19}/>{t('添加到手机主屏幕')}<ChevronDown size={16}/></summary><div className="connection-guide-content"><p>{t('像 App 一样点开 Agbrio，不用每次查找网址。添加后，再到“设置 → 消息通知”按需开启通知。')}</p>
  <div className="install-platforms" role="tablist" aria-label={t('手机系统')} onKeyDown={event=>{if(!['ArrowLeft','ArrowRight','Home','End'].includes(event.key))return;event.preventDefault();const next=event.key==='Home'?'ios':event.key==='End'?'android':platform==='ios'?'android':'ios';setPlatform(next);event.currentTarget.querySelector<HTMLButtonElement>(`[id="${prefix}-${next}"]`)?.focus();}}>{(['ios','android'] as const).map(option=><button key={option} type="button" role="tab" id={`${prefix}-${option}`} tabIndex={platform===option?0:-1} aria-selected={platform===option} aria-controls={`${prefix}-instructions`} onClick={()=>setPlatform(option)}>{option==='ios'?'iPhone / iPad':'Android'}</button>)}</div>
  <div role="tabpanel" id={`${prefix}-instructions`} aria-labelledby={`${prefix}-${platform}`}><p className="install-browser">{t(platform==='ios'?'使用 Safari':'使用 Chrome')}</p><ol className="connection-steps install-steps">{steps.map(({Icon,title,body},i)=><li key={title}><div className="install-illustration" aria-hidden><span className="install-window-bar"><i/><i/><i/></span><Icon size={28}/><span>{i+1}</span></div><div><strong>{t(title)}</strong><p>{t(body)}</p></div></li>)}</ol></div>
  <p className="install-note">{t('微信、QQ 等应用内浏览器可能没有安装入口。先复制网址，到 Safari 或 Chrome 打开；菜单位置会随系统版本略有不同。')}</p>
  <p className="install-note">{t('添加到主屏幕不会让电脑离线时仍能访问对话。电脑需保持开机、联网，Agbrio 保持运行。')}</p>
  <a className="install-source" target="_blank" rel="noreferrer" href={platform==='ios'?'https://support.apple.com/guide/iphone/iphea86e5236/ios':'https://support.google.com/chrome/answer/9658361?co=GENIE.Platform%3DAndroid'}>{t('查看系统官方说明')}<ArrowRight size={14}/></a>
 </div></details>;
}
