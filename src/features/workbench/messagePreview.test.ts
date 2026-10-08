import {expect,it} from 'vitest';
import {messagePreview} from './messagePreview';
it('shows readable Markdown summaries without changing or hiding the original source',()=>{
 const source='## **READY_FOR_REVIEW**\n\n- 完成 `demo` 流程\n[交接材料](https://demo.invalid/a)';
 expect(messagePreview(source)).toBe('READY_FOR_REVIEW 完成 demo 流程 交接材料');expect(source).toContain('https://demo.invalid/a');
 expect(messagePreview('<script>literal</script>')).toBe('<script>literal</script>');expect(messagePreview('')).toBe('');
 expect(messagePreview('**READY_FOR_REVIEW** · U01_MODEL_V1')).toBe('READY_FOR_REVIEW · U01_MODEL_V1');
});
