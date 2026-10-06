export type PrimaryFocus = "dashboard" | "workstream";

/** The primary Focus surface is mutually exclusive: Dashboard never owns a conversation. */
export function primaryFocusFor(dashboardIsFocus: boolean): PrimaryFocus {
  return dashboardIsFocus ? "dashboard" : "workstream";
}

export function primaryFocusContract(focus: PrimaryFocus) {
  return focus === "dashboard"
    ? { ariaLabel: "Dashboard overview", header: "Dashboard", ownsConversation: false, ownsComposer: false }
    : { ariaLabel: "Workstream conversation", header: "Active Workstream", ownsConversation: true, ownsComposer: true };
}
