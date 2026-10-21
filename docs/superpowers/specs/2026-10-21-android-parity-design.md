# PMS — Android parity design

**Goal:** Собрать Android-версию PMS на том же React-интерфейсе и Rust-ядре, что и desktop, с полным набором функций: хранилище через SAF, записи, TOTP, SSH, вложения, QR, импорт/экспорт, бэкапы, биометрия, обмен и настройки.

**Architecture:** Tauri 2 Android WebView + существующий React UI. Rust-ядро не меняет формат vault и криптографию. Платформенные различия изолированы в `VaultStorage`:

- desktop — Tauri fs/dialog;
- Android — `tauri-plugin-android-fs-api` поверх SAF (`DocumentFile`/`DocumentsContract`);
- URI выбранной папки/файла хранится в виде непрозрачного токена; UI показывает `displayName`, а не SAF URI.

Android-биометрия использует `tauri-plugin-biometric` (`BiometricPrompt` + Android Keystore cipher): ключ vault шифруется после биометрического подтверждения и хранится в app-private каталоге. QR-камера использует тот же `getUserMedia` + `jsQR`, что и desktop; Android WebView поддерживает камеру после системного разрешения.

**Ограничения Android:**
- SSH-ключ нельзя установить в `~/.ssh`; вместо этого доступны экспорт OpenSSH/PEM/PKCS#8 через SAF.
- На Android используется один launcher-значок PMS; динамическая смена значка через `activity-alias` не включена в стабильную сборку (выбор значка скрыт).
- SAF-провайдеры (Google Drive и др.) могут не поддерживать rename; запись vault выполняется через документ-файл, backups создаются перед сохранением.
- Минимальная версия — Android 9 (API 28).

## Этапы

1. **Каркас Android**
   - `tauri android init`, NDK/JDK, aarch64 debug APK.
   - `minSdk 28`, Info.plist-аналоги/разрешения, фирменная иконка.
2. **SAF-хранилище**
   - выбор папки и файлов, persistable permissions, чтение/запись vault, attachments, backups.
   - create/open/lock/unlock на Android.
3. **Мобильный UI**
   - safe-area, нижняя навигация 5 вкладок с «Ещё», полноэкранные detail/editor, touch targets ≥44 px.
4. **Функции**
   - QR-камера через native scanner, save-диалоги через SAF.
   - Импорт/экспорт `.pmsvault` через app-cache staging + Rust-проверку.
   - Биометрия Android через BiometricPrompt/Keystore.
   - SSH: экспорт форматов вместо install.
5. **Обмен**
   - mDNS/HTTP transfer на Android, разрешения local network/Wi-Fi.
6. **Проверки**
   - Android unit/E2E-моки, эмулятор API 35, ручная проверка на телефоне.

## Current status

- [x] Android project generated; aarch64 debug APK builds.
- [x] SAF storage with create/open/lock/unlock/save, attachments, backups.
- [x] Mobile safe-area/navigation and «Ещё» menu.
- [ ] Динамическая смена launcher-значка (стабильный activity-alias-подход требует отдельной проверки).
- [x] Android BiometricPrompt/Keystore integration in Rust.
- [x] QR-камера через WebView, SAF save pickers, SSH export.
- [x] `.pmsvault` import/export staging for Android.
- [x] Emulator biometric/camera/SAF verification; real-device providers pending.
- [x] QR-поле увеличено под мобильный экран; обмен получил UDP-broadcast fallback и ручной ввод IP.
- [ ] Android transfer discovery and manual device testing.
- [ ] App lifecycle/back-button and background-lock polish.
