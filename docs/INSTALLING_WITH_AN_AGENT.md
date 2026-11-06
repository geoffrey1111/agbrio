# Installing Agbrio with an agent on Windows

Use the official installer and verify its published SHA256 and updater signature. For a normal unattended installation use `/S`; do not add `/NS` unless the owner explicitly declined shortcuts. The installer creates Desktop and Start Menu links and repairs existing owned entries. Taskbar pinning is a Windows owner action: open Agbrio, right-click its taskbar icon and choose Pin to taskbar.

Launch the installed canonical shortcut, or use the installed `app-control/control.ps1 -Command Start`. Resolve the actual installed folder; do not guess another computer’s path. This entry uses the desktop shell so it survives the agent/terminal closing. New GUI builds also detach inherited-job launches before opening app state. Do not use a temporary extraction directory as the durable installed entry.

The GUI uses the desktop user token, matching the installer’s RunAsUser behavior. The Pi administrator backend retains its own explicit UAC preparation. No startup service/task is needed to keep Agbrio independent.

