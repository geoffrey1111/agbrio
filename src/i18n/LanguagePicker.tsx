import {Languages} from "lucide-react";
import {setLanguagePreference,t,useLanguagePreference,type LanguagePreference} from "./index";
export function LanguagePicker(){
 const preference=useLanguagePreference();
 return <label className="r2-language-picker"><Languages size={20} aria-hidden/><span>{t("语言")}</span><select aria-label={t("语言")} value={preference} onChange={event=>setLanguagePreference(event.target.value as LanguagePreference)}><option value="system">{t("跟随系统")}</option><option value="zh-CN">简体中文</option><option value="zh-TW">繁體中文</option><option value="en">English</option></select></label>;
}
