import {expect,it} from 'vitest';
import {dismissAttention,failedAttentionKey,replyAttentionKey} from './attentionDismissals';
import {readModels} from './readModelCache';
it('persists only scoped identities and uses a fresh failure identity for new attention',()=>{
 readModels.setScope('dismissal:test-device');const watch={threadId:'pi:fixture',generation:4,snapshot:{state:'FAILED',turnId:'native',itemId:'error-1'}};const id=failedAttentionKey(watch);dismissAttention(id);dismissAttention(replyAttentionKey('claude_code:fixture','request-1'));
 const saved=JSON.parse(localStorage.getItem('agbrio.dismissed-attention.v1:dismissal:test-device')!);expect(saved).toContain(id);expect(saved).not.toContain(failedAttentionKey({...watch,snapshot:{...watch.snapshot,itemId:'error-2'}}));
 readModels.setScope('dismissal:other-device');expect(localStorage.getItem('agbrio.dismissed-attention.v1:dismissal:other-device')).toBeNull();
});
