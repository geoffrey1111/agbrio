import {cleanup,fireEvent,render,screen} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {MessageDelivery} from './MessageDelivery';
afterEach(cleanup);
it('pending delivery is a small status and confirmed delivery removes it',()=>{const view=render(<MessageDelivery status="SENDING"/>);expect(screen.getByRole('status',{name:'正在发送'})).toBeInTheDocument();expect(view.container.querySelector('.v4-delivery-spinner')).not.toBeNull();expect(screen.queryByRole('button')).toBeNull();view.rerender(<MessageDelivery status="SENT"/>);expect(view.container).toBeEmptyDOMElement();});
it('uncertainty opens read-only details instead of suggesting a resend',()=>{const read=vi.fn();render(<MessageDelivery status="UNKNOWN"><button onClick={read}>检查发送记录</button></MessageDelivery>);expect(screen.queryByText('检查发送记录')).toBeNull();fireEvent.click(screen.getByRole('button',{name:'送达尚未确认'}));fireEvent.click(screen.getByRole('button',{name:'检查发送记录'}));expect(read).toHaveBeenCalledTimes(1);});
