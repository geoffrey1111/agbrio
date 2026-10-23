# Agent application operations

Settings → AI assistant exposes the current instance MCP address, connection instructions and capability discovery. Existing authenticated instance connections can discover supported operations and read app status/diagnostics. Exact conversation/Bridge identities, scopes, owner consent and existing send fences still apply. Never route by title or UI position.

Authenticated desktop/phone owner HTTP routes share application operations with the MCP boundary. An MCP bearer is not phone-owner authentication. Claude autonomous/MCP/cross-provider writes remain disabled; only existing explicit owner text/manual relays are supported.

Windows maintenance is available through packaged `app-control/control.ps1`: Status, Capabilities, Diagnostics, Start, Stop and Restart. Stop/Restart require the exact current instance identity. A current-user protected named pipe rejects remote clients; this is not a remote shell or provider-control endpoint. Lost acknowledgement never authorizes retrying a write. Diagnostics omit message content, credentials and raw logs.

Project boards: capability discovery includes `boards.cliPath`, publication instructions and the manifest schema. A desktop agent can use the installed CLI `-Command PublishBoard -Folder <absolute-folder>`. The current-user pipe pins publication to the current Host epoch; registration is idempotent by canonical folder. Only the manifest-listed presentation files become readable through paired phone APIs. Publication is not a remote MCP operation, shell, provider action or cloud upload. See [Boards](BOARDS.md).
