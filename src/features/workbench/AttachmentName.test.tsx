import {cleanup,render,screen} from '@testing-library/react';
import {afterEach,expect,it} from 'vitest';
import {AttachmentName} from './AttachmentName';
afterEach(cleanup);
it('keeps the full name for access while allowing only its stem to ellipsize',()=>{const name='很长的项目设计说明'.repeat(30)+'.pdf';render(<AttachmentName name={name}/>);const label=screen.getByTitle(name);expect(label.textContent).toBe(name);expect(label.querySelector('.v4-file-extension')).toHaveTextContent('.pdf');expect(label.querySelector('.v4-file-stem')).not.toHaveTextContent('.pdf');});
it('does not invent an extension for dotfiles or names without a suffix',()=>{const{rerender}=render(<AttachmentName name=".gitignore"/>);expect(document.querySelector('.v4-file-extension')).toBeNull();rerender(<AttachmentName name="设计稿"/>);expect(document.querySelector('.v4-file-extension')).toBeNull();});
