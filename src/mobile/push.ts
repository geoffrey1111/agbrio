import {t as uiText} from "../i18n";
import { mobileApi } from "./api";

export type PushSetupState = "UNSUPPORTED" | "NOT_REQUESTED" | "DENIED" | "SUPPORTED_NOT_SUBSCRIBED" | "UPDATING" | "SUBSCRIBED";

// A changed script URL deliberately replaces pre-receipt workers on iOS.  A
// prior Worker could render only the legacy probe path, leaving real replies
// indistinguishable from a lost notification.  This is not a capability key
// rotation: the existing PushSubscription is preserved and re-registered.
export const CURRENT_SERVICE_WORKER_URL = "/service-worker.js?build=agbrio-bridge-entry-v1";

export async function registerCurrentServiceWorker() {
  const registration = await navigator.serviceWorker.register(CURRENT_SERVICE_WORKER_URL, { scope: "/" });
  // `register()` may hand back an existing registration while its replacement
  // is still waiting. Ask the browser to revalidate now, but never treat a
  // failed revalidation as permission to rotate a subscription.
  await registration.update?.().catch(() => undefined);
  return registration;
}

function isCurrentReplyWorker(worker: ServiceWorker | null | undefined) {
  if (!worker?.scriptURL) return false;
  const expected = new URL(CURRENT_SERVICE_WORKER_URL, window.location.href);
  const active = new URL(worker.scriptURL, window.location.href);
  return active.pathname === expected.pathname && active.searchParams.get("build") === expected.searchParams.get("build");
}

function base64UrlToBytes(value: string) {
  const padded = `${value}${"=".repeat((4 - value.length % 4) % 4)}`.replace(/-/g, "+").replace(/_/g, "/");
  return Uint8Array.from(atob(padded), character => character.charCodeAt(0));
}

function usesApplicationServerKey(subscription: PushSubscription, expected: Uint8Array) {
  const current = subscription.options.applicationServerKey;
  if (!current) return false;
  const actual = new Uint8Array(current);
  return actual.length === expected.length && actual.every((byte, index) => byte === expected[index]);
}

export async function enableWebPush() {
  if (!("serviceWorker" in navigator) || !("PushManager" in window) || !("Notification" in window) || !window.isSecureContext) throw new Error(uiText("此浏览器或当前安全上下文不支持 Web Push"));
  if (Notification.permission === "denied") throw new Error(uiText("浏览器已拒绝通知权限；请在站点设置中重新允许。"));
  const registration = await registerCurrentServiceWorker();
  const permission = await Notification.requestPermission();
  if (permission !== "granted") throw new Error(uiText("未授予通知权限；不会创建订阅。"));
  const { publicKey } = await mobileApi.pushConfig();
  const applicationServerKey = base64UrlToBytes(publicKey);
  const existing = await registration.pushManager.getSubscription();
  if (existing && usesApplicationServerKey(existing, applicationServerKey)) {
    // Router has no durable receipt for this browser subscription yet. Keep a
    // valid browser subscription intact and re-register its public material.
    await mobileApi.upsertPushSubscription(existing.toJSON());
    return;
  }
  if (existing) {
    // Browsers allow one subscription per service-worker scope. A subscription
    // created under a rotated VAPID key rejects `subscribe()` with
    // InvalidStateError, so retire that exact stale endpoint before creating
    // the Router's current one. No reply content crosses this boundary.
    await mobileApi.removePushSubscription(existing.endpoint);
    if (!(await existing.unsubscribe())) throw new Error(uiText("无法注销过期的本机通知订阅。"));
  }
  const subscription = await registration.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey });
  await mobileApi.upsertPushSubscription(subscription.toJSON());
}

export async function pushSetupState(): Promise<PushSetupState> {
  if (!("serviceWorker" in navigator) || !("PushManager" in window) || !("Notification" in window) || !window.isSecureContext) return "UNSUPPORTED";
  if (Notification.permission === "denied") return "DENIED";
  if (Notification.permission === "default") return "NOT_REQUESTED";
  // This intentionally runs for already-enrolled devices.  It updates a
  // legacy Worker without asking for notification permission, rotating a VAPID
  // key, or creating a new subscription. A transient Worker-update failure
  // must not turn the whole phone workbench into a Router-unavailable screen:
  // preserve and inspect the existing scope when it remains readable.
  const registration = await registerCurrentServiceWorker().catch(() =>
    navigator.serviceWorker.getRegistration("/").catch(() => undefined),
  );
  if (!registration) return "SUPPORTED_NOT_SUBSCRIBED";
  const subscription = await registration.pushManager.getSubscription();
  if (!subscription) return "SUPPORTED_NOT_SUBSCRIBED";
  // A basic push can be handled by a legacy Worker while the real-reply JSON
  // shape is not. Do not promise business notifications until the active
  // Worker is the current reply-aware build. The existing subscription is
  // deliberately retained while the browser completes that update.
  return isCurrentReplyWorker(registration.active) ? "SUBSCRIBED" : "UPDATING";
}

export async function disableWebPush() {
  const registration = await navigator.serviceWorker.getRegistration("/");
  const subscription = await registration?.pushManager.getSubscription();
  if (!subscription) return;
  await mobileApi.removePushSubscription(subscription.endpoint);
  await subscription.unsubscribe();
}
