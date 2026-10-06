// One narrow Executor stdio child of Router. No daemon/supervisor/Agent runtime.
import { createInterface } from 'node:readline';
import path from 'node:path';
import { BrowserExecutor } from './executor.mjs';
import { ChromiumRuntime } from './native-runtime.mjs';
import { TerminalAttachments } from './attachments.mjs';

const [resourceDirectory, dataDirectory] = process.argv.slice(2);
const runtime = new ChromiumRuntime({ resourceDirectory, dataDirectory });
const executor = new BrowserExecutor({
  receiptDirectory: path.join(dataDirectory, 'browser-receipts'),
  connectionProvider: () => runtime.connection(),
});
const attachments = new TerminalAttachments(executor, path.join(dataDirectory, 'materializations'));
const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
for await (const line of input) {
  let request;
  try {
    if (Buffer.byteLength(line, 'utf8') > 1_000_000) throw new Error('EXECUTOR_REQUEST_TOO_LARGE');
    request = JSON.parse(line);
    let result;
    if (request.operation === 'quit') {
      await runtime.shutdown();
      process.stdout.write(`${JSON.stringify({ id: request.id, result: { status: 'STOPPED' } })}\n`);
      break;
    } else if (request.operation === 'owner_authentication_completed') {
      result = await runtime.ownerConfirmedAuthentication();
    } else if (request.operation === 'open') {
      result = await runtime.openForAuthentication();
    } else if (request.operation === 'receipt' && await runtime.authRequired()) {
      result = await executor._read(request.dispatchId) ?? { status: 'NOT_FOUND' };
    } else if (await runtime.authRequired()) result = { status: 'AUTH_REQUIRED' };
    else if (request.operation === 'health') result = await executor.health();
    else if (request.operation === 'binding') result = await executor.binding();
    else if (request.operation === 'verify') result = await executor.verify(request.conversationId);
    else if (request.operation === 'list_attachments') result = await attachments.list(request.conversationId, request.messageId);
    else if (request.operation === 'materialize_attachment') result = await attachments.materialize(request.conversationId, request.messageId, request.resourceId);
    else if (request.operation === 'observe') result = await executor.observe(request.conversationId, !request.passive);
    else if (request.operation === 'send') result = await executor.send(request.conversationId, request.dispatchId, request.immutablePayload, request.payloadSha256, request.attachments ?? []);
    else if (request.operation === 'receipt') result = await executor.receipt(request.dispatchId);
    else throw new Error('EXECUTOR_OPERATION_INVALID');
    if (result.status === 'AUTH_REQUIRED' || result.code === 'AUTH_REQUIRED' || result.authRequired) {
      await runtime.latchAuthRequired();
      result = { ...result, authRequired: true };
    }
    process.stdout.write(`${JSON.stringify({ id: request.id, result })}\n`);
  } catch (error) {
    const code = error.code ?? (/^[A-Z][A-Z0-9_]+$/.test(error.message) ? error.message : 'EXECUTOR_OPERATION_FAILED');
    process.stdout.write(`${JSON.stringify({ id: request?.id, result: { status: 'UNAVAILABLE', code } })}\n`);
  }
}
// EOF means Router has exited. The owned browser is not promised immortal.
await runtime.shutdown();
