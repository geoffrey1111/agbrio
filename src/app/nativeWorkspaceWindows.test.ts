import { describe, expect, it } from "vitest";
import { currentRouterWindowContext } from "./nativeWorkspaceWindows";

describe("native Router window contexts", () => {
  it("keeps a focused Workstream and its observed response pinned in its own URL context", () => {
    expect(currentRouterWindowContext("?view=workspace&workstream=workstream-a&response=response-a")).toEqual({
      view: "workspace", workstreamId: "workstream-a", responseIdentity: "response-a",
    });
  });

  it("does not let aggregate provider workspace URLs inherit a selected Workstream", () => {
    expect(currentRouterWindowContext("?view=codex-workspace&workstream=workstream-a")).toEqual({ view: "codex-workspace", workstreamId: undefined, responseIdentity: undefined });
    expect(currentRouterWindowContext("?view=decision-workspace&workstream=workstream-b")).toEqual({ view: "decision-workspace", workstreamId: undefined, responseIdentity: undefined });
  });

  it("opens Router Board as its own global native presentation context", () => {
    expect(currentRouterWindowContext("?view=dashboard&workstream=workstream-a")).toEqual({
      view: "dashboard", workstreamId: undefined, responseIdentity: undefined,
    });
  });

  it("allows only the isolated workflow acceptance fixture in a native URL", () => {
    expect(currentRouterWindowContext("?view=decision-workspace&visualFixture=workflow-acceptance")).toEqual({
      view: "decision-workspace", workstreamId: undefined, responseIdentity: undefined, visualFixture: "workflow-acceptance",
    });
    expect(currentRouterWindowContext("?view=decision-workspace&visualFixture=anything-else").visualFixture).toBeUndefined();
  });
});
