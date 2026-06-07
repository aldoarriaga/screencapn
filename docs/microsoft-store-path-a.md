# Microsoft Store Path A Readiness

Path A means Screen Cap'n launches through Microsoft Store first. Microsoft Store signing is the trusted distribution path for v1. Direct GitHub or website downloads are deferred until a trusted direct-download signing option is chosen.

## Release Defaults

- App name: Screen Cap'n
- Distribution: Microsoft Store MSIX first
- Direct download: deferred
- Minimum Windows version: Windows 10 version 1809 / build 17763 or later
- Architecture for first submission: x64
- Store privacy policy: use `PRIVACY.md` as the source copy, then publish it at a public URL before submission
- Support URL: TODO
- Website URL: TODO
- Store category: Productivity

## Store Submission Checklist

- Reserve the app name in Partner Center.
- Create a Microsoft Store submission package using MSIX tooling.
- Upload the Store package through Partner Center.
- Run Windows App Certification Kit before submission.
- Use Microsoft Store signing for the Store package.
- Prepare listing text, screenshots, icon assets, privacy URL, support URL, and age rating.
- Include certification notes:
  - Screen Cap'n captures screenshots only after user action.
  - Screen Cap'n uses a global hotkey for capture.
  - Clipboard copy is user-triggered.
  - Screenshot content is processed locally and is not uploaded by the app.

## Local Release Check

Run:

```powershell
.\scripts\store-release-check.ps1
```

Use `-SkipAudit` only when offline:

```powershell
.\scripts\store-release-check.ps1 -SkipAudit
```

The script validates the local build and dependency surface. It does not create or upload the MSIX package because package identity, publisher identity, Store reservation, and WACK execution are machine/account-specific.

## Manual Store Tasks

- Create or confirm Partner Center developer account.
- Reserve "Screen Cap'n" or choose the closest approved Store name.
- Create package identity and publisher identity.
- Generate Store visual assets.
- Capture Store screenshots.
- Publish `PRIVACY.md` as a public privacy policy URL.
- Run WACK against the generated package.
- Submit through Partner Center.
