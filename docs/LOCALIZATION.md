# Application languages

English (`en`), Simplified Chinese (`zh-CN`), Traditional Chinese (`zh-TW`) and
system default are available in Settings. Unsupported system languages use English.
The primary system language determines the default; Traditional Chinese includes
Hant, Taiwan, Hong Kong and Macau. Desktop and phone preferences are per origin/device.
Pairing offers the same selector before authentication.

`src/i18n/en.json` and `zh-TW.json` contain application copy, keyed by its original
Chinese wording. Components explicitly mark chrome with `t`; source messages,
names, file content, review drafts, native IDs and outgoing instructions retain
their original bytes. Switching updates mounted readers and dialogs without
remounting the application. Form errors and application status labels also update.
Date/time formatting uses the chosen locale and the reader's local timezone.

`locale.ts` and `startupCopy.ts` keep the boot shell independent of React/full
catalogs. Tests check boot/catalog parity, interpolation slots, state preservation,
original-content preservation, persisted preference, cross-tab synchronization,
and recovery after verification failure.

Japanese/Korean are currently README translations, not selectable app languages.
OS-native menus and provider-produced content are not machine-translated.
Browser geometry validation with fixture APIs is not iPhone physical-device UAT.
