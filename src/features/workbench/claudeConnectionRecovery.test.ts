import {describe,expect,it} from 'vitest';
import {claudeCompletionInstructions,claudeFailureStage,claudeManualInstallPowerShell,claudeRecoveryKind} from './claudeConnectionRecovery';
describe('Claude setup recovery',()=>{
 it('shows fixed stage metadata without rendering arbitrary native error text',()=>{
  expect(claudeFailureStage('CLAUDE_PLUGIN_ADMIN_REFUSED:SAVE_CONFIGURATION:125')).toContain('125');
  expect(claudeFailureStage('CLAUDE_PLUGIN_ADMIN_REFUSED:private-value:secret')).toBeNull();
  expect(claudeFailureStage('CLAUDE_PLUGIN_ADMIN_REFUSED:UNRECOGNIZED:1')).not.toContain('UNRECOGNIZED');
  expect(claudeRecoveryKind('CLAUDE_CONNECTION_BUSY')).toBe('wait');
  expect(claudeRecoveryKind('CLAUDE_PLUGIN_DIFFERENT_HOST')).toBe('host');
  expect(claudeRecoveryKind('CLAUDE_DESKTOP_ENGINE_TOO_OLD')).toBe('update');
  expect(claudeRecoveryKind('CLAUDE_PLUGIN_ADMIN_REFUSED:SAVE_CONFIGURATION:1')).toBe('configuration');
 });
 for(const lang of ['zh-CN','zh-TW','en'] as const)it(`keeps reload, same-session verification and exact opt-in notices distinct (${lang})`,()=>{
  const text=claudeCompletionInstructions(lang);
  for(const token of ['agbrio_read_app','codex_wait','WAITING','threadId/generation/turnId','/reload-plugins'])expect(text).toContain(token);
  expect(text).not.toContain('prompt.submit');
 });
 it('ships a local-only manual installer with modern and legacy roots and no credential configuration',()=>{
  for(const root of ['Claude-Data/claude_pzs8sxrjxfjjc','LocalCache/Roaming/Claude','marketplace','agbrio-bridge@agbrio-observation'])expect(claudeManualInstallPowerShell).toContain(root);
  expect(claudeManualInstallPowerShell).not.toMatch(/Invoke-WebRequest|Invoke-RestMethod|ExecutionPolicy|--values-stdin|Stop-Process|Set-ExecutionPolicy/);
 });
});
