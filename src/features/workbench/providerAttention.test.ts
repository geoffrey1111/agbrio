import {describe,it,expect} from 'vitest';
import {providerOwnerAttention} from './providerAttention';
describe('provider owner attention',()=>{
 it('distinguishes billing/quota/auth from transient execution failures and always gives a next step',()=>{
  expect(providerOwnerAttention('PI_PROVIDER_BILLING_REQUIRED')?.reason).toContain('余额不足');
  expect(providerOwnerAttention('PI_PROVIDER_BILLING_REQUIRED')?.next).toContain('充值');
  expect(providerOwnerAttention('PI_PROVIDER_QUOTA_EXHAUSTED')?.reason).toContain('额度已用尽');
  expect(providerOwnerAttention('PI_PROVIDER_AUTHENTICATION_ERROR')?.reason).toContain('认证失败');
  for(const code of ['PI_PROVIDER_RATE_LIMIT','PI_PROVIDER_CONNECTION_ERROR','PI_PROVIDER_ERROR',null])expect(providerOwnerAttention(code)).toBeNull();
 });
});
