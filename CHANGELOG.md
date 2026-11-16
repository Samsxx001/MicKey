# Changelog

## 1.3.0

- Add a Check for updates command in settings and the tray menu.
- Check stable releases from this repository when requested; no background network polling.
- Verify downloaded executable size and GitHub's SHA-256 digest before replacement.
- Restart after confirmation, preserve shortcut settings, and retain the previous executable as `.previous`.
- Add Windows CI tests and automated GitHub Release downloads.

## 1.2.0

- Close to the system tray while keeping microphone shortcuts active.
- Keep a taskbar fallback when the tray is unavailable; add an explicit Exit button.

## 1.1.0

- Add green, red/slashed, and unavailable microphone status icons.
- Follow Windows Core Audio notifications without polling.

## 1.0.0

- Add configurable microphone mute shortcuts and a native Windows settings window.
