import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,within} from '@testing-library/react';
import {MarkdownMessage} from './MarkdownMessage';
import {csvRows} from './FilePreview';
afterEach(cleanup);
const file=(filename:string,text:string)=>({filename,mime:'application/octet-stream',data:btoa(unescape(encodeURIComponent(text)))});
it('opens a UTF8 Markdown document and scopes its relative image to that document',async()=>{
 const media=vi.fn(async(source:string)=>decodeURI(source)==='docs/指南.md'?file('指南.md','# 阅读标题\n\n![diagram](pic.png)'):{filename:'pic.png',mime:'image/png',data:'fixture'});
 render(<MarkdownMessage text='[指南](docs/指南.md)' media={media}/>);fireEvent.click(screen.getByRole('link',{name:'指南'}));
 const dialog=await screen.findByRole('dialog',{name:'指南.md'});expect(await within(dialog).findByRole('heading',{name:'阅读标题'})).toBeInTheDocument();await within(dialog).findByRole('img');expect(media).toHaveBeenCalledWith('pic.png',undefined,[encodeURI('docs/指南.md')]);
 fireEvent.click(within(dialog).getByRole('button',{name:'关闭预览'}));expect(screen.queryByRole('dialog')).not.toBeInTheDocument();expect(screen.getByRole('link',{name:'指南'})).toBeInTheDocument();
});
it('preserves the open document snapshot during message refresh',async()=>{const media=vi.fn(async()=>file('a.txt','original bytes'));const view=render(<MarkdownMessage text='[open](a.txt)' media={media}/>);fireEvent.click(screen.getByRole('link'));await screen.findByText('original bytes');view.rerender(<MarkdownMessage text='later reply' media={media}/>);expect(screen.getByText('original bytes')).toBeInTheDocument();expect(media).toHaveBeenCalledTimes(1);});
it('renders JSON as inert text and reports unavailable files',async()=>{const media=vi.fn(async()=>file('a.json','{"html":"<script>alert(1)</script>"}'));render(<MarkdownMessage text='[json](a.json)' media={media}/>);fireEvent.click(screen.getByRole('link'));await screen.findByRole('dialog',{name:'a.json'});expect(document.querySelector('script')).toBeNull();cleanup();render(<MarkdownMessage text='[missing](a.txt)' media={vi.fn().mockRejectedValue(Error('missing'))}/>);fireEvent.click(screen.getByRole('link'));expect(await screen.findByRole('alert')).toHaveTextContent('文件暂不可用');});
it('parses quoted CSV multiline fields, escaped quotes and CRLF with bounds',()=>{expect(csvRows('a,b\r\n"one,two","line1\nline2"\r\n"a""b",end')).toEqual([['a','b'],['one,two','line1\nline2'],['a"b','end']]);expect(()=>csvRows('"incomplete')).toThrow();expect(()=>csvRows('x\n'.repeat(2002))).toThrow();});
it('renders CSV cells without executing formulas or markup',async()=>{render(<MarkdownMessage text='[table](table.csv)' media={vi.fn(async()=>file('table.csv','name,value\n<script>,=HYPERLINK("x")'))}/>);fireEvent.click(screen.getByRole('link'));expect(await screen.findByRole('cell',{name:'<script>'})).toBeInTheDocument();expect(document.querySelector('script')).toBeNull();});
