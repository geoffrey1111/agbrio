import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {NewConversationForm} from './NewConversationForm';
import {readModels} from './readModelCache';
import type {ConversationCreationApi,CreationInput} from './conversationCreationApi';
afterEach(()=>{cleanup();localStorage.clear();readModels.setScope('');});
function fixture():ConversationCreationApi{return{options:vi.fn(async()=>({providers:[{id:'CODEX',label:'Codex',available:true},{id:'CLAUDE_CODE',label:'Claude Code',available:false},{id:'ANTIGRAVITY',label:'Antigravity',available:false}],projects:['D:/fixture'],models:[{model:'a',displayName:'A',supportedReasoningEfforts:[{reasoningEffort:'high'}]},{model:'b',displayName:'B',supportedReasoningEfforts:[{reasoningEffort:'low'}]}]})),create:vi.fn(),receipt:vi.fn()};}
async function fill(){await waitFor(()=>expect(screen.getByLabelText('Agent 平台')).toBeEnabled());fireEvent.change(screen.getByLabelText('项目目录'),{target:{value:'D:/fixture'}});fireEvent.change(screen.getByLabelText('第一条指令'),{target:{value:'fixture instruction'}});}
it('keeps the original request after transport loss and remount; checking never recreates or sends',async()=>{
 readModels.setScope('creation-transport-loss');const api=fixture();vi.mocked(api.create).mockRejectedValue(new Error('transport failure'));vi.mocked(api.receipt).mockResolvedValue({requestId:'original',status:'UNKNOWN',nativeId:'native-original'});
 const view=render(<NewConversationForm api={api} onCreated={vi.fn()}/>);await fill();fireEvent.click(screen.getByRole('button',{name:'创建并发送'}));await screen.findByRole('alert');expect(api.create).toHaveBeenCalledTimes(1);
 const original=vi.mocked(api.create).mock.calls[0][0] as CreationInput;view.unmount();render(<NewConversationForm api={api} onCreated={vi.fn()}/>);
 expect(screen.getByLabelText('第一条指令')).toHaveValue('fixture instruction');expect(screen.getByLabelText('第一条指令')).toBeDisabled();fireEvent.click(screen.getByRole('button',{name:'检查本次创建回执'}));
 await waitFor(()=>expect(api.receipt).toHaveBeenCalledWith(original.requestId));expect(api.create).toHaveBeenCalledTimes(1);await screen.findByText(/原对话已创建/);
});
it('uses the selected model efforts and never permits unavailable platforms',async()=>{
 const api=fixture();render(<NewConversationForm api={api} onCreated={vi.fn()}/>);await fill();expect(screen.getByRole('option',{name:'Antigravity（暂不可用）'})).toBeEnabled();
 fireEvent.change(screen.getByLabelText('模型'),{target:{value:'a'}});fireEvent.change(screen.getByLabelText('推理强度'),{target:{value:'high'}});fireEvent.change(screen.getByLabelText('模型'),{target:{value:'b'}});
 expect(screen.getByLabelText('推理强度')).toHaveValue('');expect(screen.queryByRole('option',{name:'high'})).toBeNull();expect(screen.getByRole('option',{name:'low'})).toBeVisible();expect(api.create).not.toHaveBeenCalled();
});
it('does not mix model catalogs between providers',async()=>{
 const api=fixture();const options=await api.options();options.providers[1]={id:'CLAUDE_CODE',label:'Claude Code',available:true,models:[{model:'claude-native',displayName:'Native Claude',supportedReasoningEfforts:[{reasoningEffort:'max'}]}]};vi.mocked(api.options).mockResolvedValue(options);
 render(<NewConversationForm api={api} onCreated={vi.fn()}/>);await fill();fireEvent.change(screen.getByLabelText('模型'),{target:{value:'a'}});
 fireEvent.change(screen.getByLabelText('Agent 平台'),{target:{value:'CLAUDE_CODE'}});expect(screen.getByLabelText('模型')).toHaveValue('');expect(screen.queryByRole('option',{name:'A'})).toBeNull();expect(screen.getByRole('option',{name:'Native Claude'})).toBeVisible();
 fireEvent.change(screen.getByLabelText('模型'),{target:{value:'claude-native'}});expect(screen.getByRole('option',{name:'max'})).toBeVisible();expect(screen.queryByRole('option',{name:'high'})).toBeNull();
});

it('recovers unavailable options without losing the draft or creating a conversation',async()=>{
 const api=fixture();const ready=await api.options();const unavailable={...ready,providers:ready.providers.map(p=>p.id==='CODEX'?{...p,available:false,reason:'CODEX_BACKEND_BUSY_RETRY',models:[]}:p),models:[]};
 vi.mocked(api.options).mockReset().mockResolvedValueOnce(unavailable).mockResolvedValue(ready);
 render(<NewConversationForm api={api} onCreated={vi.fn()}/>);
 await screen.findByText(/CODEX_BACKEND_BUSY_RETRY/);fireEvent.change(screen.getByLabelText('第一条指令'),{target:{value:'keep draft'}});
 fireEvent.click(screen.getByRole('button',{name:'重新读取平台和模型'}));await screen.findByRole('option',{name:'A'});
 expect(screen.getByLabelText('第一条指令')).toHaveValue('keep draft');expect(api.create).not.toHaveBeenCalled();
});
it('explains unavailable platforms without mixing the Codex catalog into them',async()=>{
 const api=fixture();render(<NewConversationForm api={api} onCreated={vi.fn()}/>);await fill();
 fireEvent.change(screen.getByLabelText('Agent 平台'),{target:{value:'ANTIGRAVITY'}});
 expect(screen.queryByRole('option',{name:'A'})).toBeNull();expect(screen.getByRole('button',{name:'创建并发送'})).toBeDisabled();expect(screen.getByText(/当前平台尚未就绪/)).toBeVisible();
});
