import { describe, expect, it } from "vitest";
import { primaryFocusContract, primaryFocusFor } from "./focusSurface";

describe("primary Focus surface", () => {
  it("removes the Dashboard primary surface before the Workstream conversation becomes final", () => {
    const dashboard = primaryFocusContract(primaryFocusFor(true));
    const workstream = primaryFocusContract(primaryFocusFor(false));

    expect(dashboard).toMatchObject({ ariaLabel: "Dashboard overview", header: "Dashboard", ownsConversation: false, ownsComposer: false });
    expect(workstream).toMatchObject({ ariaLabel: "Workstream conversation", header: "Active Workstream", ownsConversation: true, ownsComposer: true });
  });
});
