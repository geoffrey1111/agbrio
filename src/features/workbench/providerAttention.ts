/** Fixed provider failures requiring an owner action, keyed only by the
 * backend's classified code; the card never infers a cause from prose. */
export type ProviderOwnerAttention={reason:string;next:string};
export function providerOwnerAttention(code:string|null|undefined):ProviderOwnerAttention|null{
 switch(code){
  case 'PI_PROVIDER_BILLING_REQUIRED':return {reason:'Pi 的上游 API 余额不足',next:'去上游 API 平台充值，再打开对话继续任务。'};
  case 'PI_PROVIDER_QUOTA_EXHAUSTED':return {reason:'Pi 的上游 API 额度已用尽',next:'在上游账户调高额度或等额度重置，再打开对话继续任务。'};
  case 'PI_PROVIDER_AUTHENTICATION_ERROR':return {reason:'Pi 的上游 API 认证失败',next:'检查 Pi 的 API 配置里的密钥或授权，再打开对话继续任务。'};
  default:return null;
 }
}
