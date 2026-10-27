import {t} from "../../i18n";
export const connectionError=(error:unknown)=>t(({
 CLAUDE_DESKTOP_ENGINE_NOT_FOUND:"未找到 Claude Code 内置引擎。请先打开 Claude 桌面应用的 Code 本地会话，再重试；若已能使用，请提供 /status 版本和引擎路径。",
 CLAUDE_DESKTOP_ENGINE_TOO_OLD:"Claude Code 内置引擎版本低于 2.1.286，请更新 Claude 桌面应用后重试。",
 CLAUDE_PLUGIN_DIFFERENT_HOST:"这个插件已连接另一个 Agbrio 地址，请先核对原连接。",
 CLAUDE_CONNECTION_BUSY:"连接正在配置，请稍候。",
 CLAUDE_PLUGIN_ADMIN_TIMEOUT:"配置超时，未确认连接成功。请稍后检查连接。",
 CLAUDE_PLUGIN_ADMIN_REFUSED:"Claude 拒绝了插件配置；原任务未启动或重启，请检查插件权限。",
 CLAUDE_PLUGIN_UPDATE_REQUIRED:"Claude 插件仍是旧版，更新尚未确认。请检查插件更新后重试。",
 CLAUDE_PLUGIN_VERSION_UNCONFIRMED:"无法确认 Claude 插件版本，尚未启用新的连接。",
 CLAUDE_PLUGIN_ENABLE_UNCONFIRMED:"Claude 插件尚未确认启用，请检查插件状态。",
} as Record<string,string>)[String(error)]??"连接配置未完成，请检查 Claude 是否已安装；也可以展开手动连接。");
