import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  mobileApi: {
    pushConfig: vi.fn(),
    upsertPushSubscription: vi.fn(),
    removePushSubscription: vi.fn(),
  },
}));

import { mobileApi } from "./api";
import { CURRENT_SERVICE_WORKER_URL, enableWebPush, pushSetupState } from "./push";

describe("enableWebPush", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("replaces only a stale browser subscription whose VAPID key no longer matches Router", async () => {
    const stale = {
      endpoint: "https://push.example/stale",
      options: { applicationServerKey: new Uint8Array([1, 2, 3]).buffer },
      unsubscribe: vi.fn().mockResolvedValue(true),
      toJSON: vi.fn(),
    };
    const fresh = { endpoint: "https://push.example/fresh", toJSON: vi.fn(() => ({ endpoint: "https://push.example/fresh" })) };
    const subscribe = vi.fn().mockResolvedValue(fresh);
    const registration = { pushManager: { getSubscription: vi.fn().mockResolvedValue(stale), subscribe } };
    Object.defineProperty(navigator, "serviceWorker", { configurable: true, value: { register: vi.fn().mockResolvedValue(registration) } });
    Object.defineProperty(window, "isSecureContext", { configurable: true, value: true });
    Object.defineProperty(window, "PushManager", { configurable: true, value: class {} });
    vi.stubGlobal("PushManager", class {});
    Object.defineProperty(window, "Notification", { configurable: true, value: { permission: "granted", requestPermission: vi.fn().mockResolvedValue("granted") } });
    vi.stubGlobal("Notification", window.Notification);
    vi.mocked(mobileApi.pushConfig).mockResolvedValue({ publicKey: "BAUG" });

    await enableWebPush();

    expect(navigator.serviceWorker.register).toHaveBeenCalledWith(CURRENT_SERVICE_WORKER_URL, { scope: "/", updateViaCache: "none" });
    expect(mobileApi.removePushSubscription).toHaveBeenCalledWith("https://push.example/stale");
    expect(stale.unsubscribe).toHaveBeenCalledOnce();
    expect(subscribe).toHaveBeenCalledWith({ userVisibleOnly: true, applicationServerKey: new Uint8Array([4, 5, 6]) });
    expect(mobileApi.upsertPushSubscription).toHaveBeenCalledWith({ endpoint: "https://push.example/fresh" });
  });

  it("updates the current Worker during a normal already-subscribed status check", async () => {
    const subscription = { endpoint: "https://push.example/current" };
    const registration = {
      active: { scriptURL: `https://router.example${CURRENT_SERVICE_WORKER_URL}` },
      pushManager: { getSubscription: vi.fn().mockResolvedValue(subscription) },
      update: vi.fn().mockResolvedValue(undefined),
    };
    const register = vi.fn().mockResolvedValue(registration);
    Object.defineProperty(navigator, "serviceWorker", { configurable: true, value: { register } });
    Object.defineProperty(window, "isSecureContext", { configurable: true, value: true });
    Object.defineProperty(window, "PushManager", { configurable: true, value: class {} });
    vi.stubGlobal("PushManager", class {});
    Object.defineProperty(window, "Notification", { configurable: true, value: { permission: "granted" } });
    vi.stubGlobal("Notification", window.Notification);

    await expect(pushSetupState()).resolves.toBe("SUBSCRIBED");
    expect(register).toHaveBeenCalledWith(CURRENT_SERVICE_WORKER_URL, { scope: "/", updateViaCache: "none" });
    expect(registration.update).toHaveBeenCalledOnce();
  });

  it("does not call a legacy Worker business-notification ready while preserving its subscription", async () => {
    const subscription = { endpoint: "https://push.example/current" };
    const registration = {
      active: { scriptURL: "https://router.example/service-worker.js" },
      pushManager: { getSubscription: vi.fn().mockResolvedValue(subscription) },
      update: vi.fn().mockResolvedValue(undefined),
    };
    Object.defineProperty(navigator, "serviceWorker", { configurable: true, value: { register: vi.fn().mockResolvedValue(registration) } });
    Object.defineProperty(window, "isSecureContext", { configurable: true, value: true });
    Object.defineProperty(window, "PushManager", { configurable: true, value: class {} });
    vi.stubGlobal("PushManager", class {});
    Object.defineProperty(window, "Notification", { configurable: true, value: { permission: "granted" } });
    vi.stubGlobal("Notification", window.Notification);

    await expect(pushSetupState()).resolves.toBe("UPDATING");
    expect(registration.pushManager.getSubscription).toHaveBeenCalledOnce();
  });

  it("keeps an existing subscription legible when the Worker upgrade is temporarily unavailable", async () => {
    const subscription = { endpoint: "https://push.example/current" };
    const existing = { active: { scriptURL: `https://router.example${CURRENT_SERVICE_WORKER_URL}` }, pushManager: { getSubscription: vi.fn().mockResolvedValue(subscription) } };
    Object.defineProperty(navigator, "serviceWorker", {
      configurable: true,
      value: {
        register: vi.fn().mockRejectedValue(new Error("temporary worker update failure")),
        getRegistration: vi.fn().mockResolvedValue(existing),
      },
    });
    Object.defineProperty(window, "isSecureContext", { configurable: true, value: true });
    Object.defineProperty(window, "PushManager", { configurable: true, value: class {} });
    vi.stubGlobal("PushManager", class {});
    Object.defineProperty(window, "Notification", { configurable: true, value: { permission: "granted" } });
    vi.stubGlobal("Notification", window.Notification);

    await expect(pushSetupState()).resolves.toBe("SUBSCRIBED");
    expect(navigator.serviceWorker.getRegistration).toHaveBeenCalledWith("/");
  });
});
