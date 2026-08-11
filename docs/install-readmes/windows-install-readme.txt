physics-IDE Experimental Windows Install Readme

Status
- This Windows build is experimental and unsigned.
- SmartScreen warnings are expected on first launch.
- Official supported release target is Linux (.deb).

What to Download
- physics-ide Windows executable package from the GitHub release assets.
- Keep all files from the package together in one folder.

Install Steps (Unsigned Build)
1. Download the Windows asset.
2. If downloaded as a .zip archive, right-click it and choose Extract All.
3. Open the extracted folder.
4. Double-click the physics-IDE executable.

If SmartScreen Blocks Launch
1. In the "Windows protected your PC" dialog, click More info.
2. Click Run anyway.
3. If Windows asks for permission, click Yes.

Optional Unblock Step (Sometimes Needed)
- If files are flagged as downloaded from the internet:
1. Right-click the .exe file.
2. Open Properties.
3. On the General tab, check Unblock (if present).
4. Click Apply, then OK.
5. Launch again.

First Run Notes
- Initial startup may take longer while app dependencies initialize.
- If a firewall prompt appears, allow network access if you plan to use OpenAI or Gemini provider calls.

Known Experimental Limitations
- No Windows code-signing certificate is attached.
- Security prompts can reappear after updates.
- Antivirus products may perform additional scans before first launch.

Quick Troubleshooting
- App does not start:
  - Re-extract the archive to a normal user folder (for example, Desktop or Documents).
  - Confirm required files are still beside the executable.
- SmartScreen still blocks:
  - Repeat More info -> Run anyway.
  - Use the Properties -> Unblock step.
- Network/provider calls fail:
  - Verify API keys in app settings.
  - Check firewall or corporate proxy restrictions.

Safety Reminder
- Only install binaries downloaded from the official physics-IDE GitHub repository and release page.

Support Scope
- Linux (.deb) is the official supported release channel.
- Windows is currently provided as an experimental convenience build.
