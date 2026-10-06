import "@testing-library/jest-dom/vitest";

Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: (query: string) => ({ matches: false, media: query, onchange: null, addEventListener: () => undefined, removeEventListener: () => undefined, addListener: () => undefined, removeListener: () => undefined, dispatchEvent: () => false }),
});

class ResizeObserverStub { observe() {} unobserve() {} disconnect() {} }
Object.defineProperty(window, "ResizeObserver", { writable: true, value: ResizeObserverStub });

// Existing fixtures explicitly exercise Simplified Chinese; locale cases override it.
Object.defineProperty(navigator,"languages",{configurable:true,value:["zh-CN"]});
