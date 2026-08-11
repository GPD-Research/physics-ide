physics-IDE Experimental macOS Install Readme

Status
- This macOS build is experimental and unsigned/unnotarized.
- Gatekeeper warnings are expected on first launch.
- Official supported release target is Linux (.deb).

What to Download
- physics-IDE macOS app asset from the GitHub release assets.
- Keep the app bundle intact after download.

Install Steps (Unsigned Build)
1. Download the macOS release asset.
2. If the asset is an archive, extract it.
3. Move Physics IDE.app to /Applications (recommended).
4. Try launching once from Finder.

If Gatekeeper Blocks Launch
Method A: Finder Open Override
1. In Finder, right-click Physics IDE.app.
2. Click Open.
3. In the warning dialog, click Open again.

Method B: System Settings Override
1. Try opening the app once so macOS records the block.
2. Open System Settings -> Privacy & Security.
3. Scroll to the Security section.
4. Click Open Anyway for Physics IDE.app.
5. Confirm Open in the follow-up prompt.

Optional Terminal Method (Advanced Users)
- If quarantine flags continue blocking launch, you can remove the quarantine attribute.
- Run this command in Terminal:
  xattr -dr com.apple.quarantine "/Applications/Physics IDE.app"
- Then try opening the app again.

First Run Notes
- Initial startup can take extra time while macOS verifies and caches app files.
- If prompted for network access, allow as needed for provider API usage.

Known Experimental Limitations
- App is not signed with an Apple Developer certificate.
- App is not notarized.
- Future macOS updates may tighten launch restrictions for unsigned apps.

Quick Troubleshooting
- App bounces and closes immediately:
  - Move the app to /Applications.
  - Re-download and replace corrupted copies.
- "Damaged" or blocked message persists:
  - Re-run Finder right-click Open flow.
  - Use the Privacy & Security Open Anyway action.
  - Use the quarantine removal command if needed.
- Provider requests fail in app:
  - Verify API key settings.
  - Check firewall, VPN, or proxy restrictions.

Safety Reminder
- Only install binaries downloaded from the official physics-IDE GitHub repository and release page.

Support Scope
- Linux (.deb) is the official supported release channel.
- macOS is currently provided as an experimental convenience build.
