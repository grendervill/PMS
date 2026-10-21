# PMS — биометрия, QR и импорт/экспорт

Дата: 2026-10-21
Статус: проект дизайна, ожидает подтверждения
Область: macOS-этап. Android-биометрия, камера и SAF — следующий этап.

## 1. Зачем

Добавить в PMS три группы возможностей до начала Android-этапа:

1. Разблокировку хранилища по отпечатку пальца на macOS (Touch ID) с настройкой
   автоматического запроса при запуске; на Android — та же модель поведения
   позже.
2. QR-коды: сканирование TOTP-секретов, передача карточки между устройствами PMS
   и отображение TOTP в виде QR.
3. Полноценный импорт хранилищ из других менеджеров и экспорт/импорт самой PMS.

Дополнительно: несколько URL в одной записи, потому что этого требуют импорт,
QR и повседневное использование.

## 2. Границы

### Входит

- macOS Touch ID: включение, выключение, авторазблокировка при запуске,
  5 попыток, откат к пароль-фразе.
- Настройки безопасности в приложении.
- Сканирование QR из файла/скриншота и с камеры, если камера доступна.
- Передача карточки PMS через QR: имя, логин, пароль, несколько URL,
  комментарий, TOTP. Без вложений и SSH.
- Сканирование TOTP QR и показ QR для существующего TOTP.
- Несколько URL в записи.
- Импорт в текущее открытое хранилище:
  Bitwarden (JSON/CSV/ZIP/encrypted JSON), 1Password (1PUX/CSV),
  KeePass (KDBX/XML/CSV), KeePassX (KDBX/CSV), Kaspersky Password Manager
  (TXT/CSV), браузеры (Chrome/Chromium/Edge/Brave/Firefox/Safari CSV),
  плюс generic CSV с ручным сопоставлением колонок.
- Полный экспорт/импорт PMS в `.pmsvault` (ZIP с зашифрованными файлами).
- Сохранение вложений, TOTP, SSH-ключей, карт, заметок и identity из импорта
  в доступном для PMS виде. Неподдерживаемое попадает в отчёт, а не теряется
  молча.
- Тесты и визуальная проверка.

### Не входит

- Android APK, Android Keystore/BiometricPrompt, Android-камера и SAF.
- Вложения и SSH в QR-передаче карточки.
- Синхронизация, облако, сервер.
- Passkeys: PMS их не поддерживает; при импорте они попадают в отчёт как
  пропущенные.
- Запись в форматы других менеджеров. Экспорт — только `.pmsvault`.
- Восстановление забытого пароля/биометрии.

## 3. Решения

1. **Несколько URL.** В записи хранится массив `urls`; первая ссылка — основная.
   Старое поле `url` остаётся для совместимости и зеркалит первую ссылку.
   Пользователь добавляет любое число строк кнопкой `+`.
2. **Биометрия — локальная, не переносится с папкой.** Ключ хранилища хранится
   в macOS Keychain с доступом по текущему набору отпечатков. На новом
   устройстве его нет; вход — пароль-фраза/ключ-файл. Идентичность записи —
   `vaultId`: копия того же vault на том же Mac использует то же локальное
   включение, потому что это тот же vault key; на другом Mac Keychain-элемента
   нет. Это осознанное решение, а не перенос биометрии вместе с папкой.
3. **5 попыток.** После каждой неудачи показываем, сколько осталось. После
   пятой или отмены системного запроса — сразу обычный ввод пароля. Кнопка
   «Повторить» появляется между попытками, чтобы не спамить системный диалог.
4. **Авторазблокировка.** Галочка «Всегда разблокировать при помощи биометрии»
   включает и запись ключа, и автоматический запрос при запуске. Отмена
   системного запроса ведёт на пароль. Настройка локальная для устройства.
5. **QR-карточка — только PMS.** Формат не совместим с другими приложениями.
   Вложения и SSH не передаются. По умолчанию QR без пароля; пользователь
   может включить парольную фразу и ввести её на принимающем устройстве.
6. **QR-сканирование.** На Mac доступны оба способа: камера и изображение/
   скриншот. Если камеры нет или доступ запрещён, остаётся изображение.
7. **Импорт — полный, в текущее хранилище.** Сначала предпросмотр и отчёт.
   Точные дубликаты по умолчанию пропускаются; можно импортировать всё.
8. **Зашифрованные импорты.** KDBX — с паролем (и опциональным key-файлом);
   Bitwarden password-protected JSON — с паролем. Account-restricted Bitwarden
   JSON нерасшифровываем: нужен password-protected экспорт.
9. **Экспорт PMS** — один `.pmsvault` ZIP с `pms.vault`, `attachments/` и
   manifest/checksums. Импорт — только в новую/пустую папку, без перезаписи.
   На чистой установке импорт `.pmsvault` доступен прямо с экрана приветствия,
   без создания временного хранилища.
10. **Настройки.** Чекбокс биометрии и импорт/экспорт живут в разделе
    «Настройки»; отдельного раздела импорта в навигации не будет.
11. **QR-ёмкость.** Используем QR версии не выше 40 с коррекцией ошибок M.
    Проверяем итоговую строку `PMSQR1:`/`PMSQR1E:` целиком: не более
    2300 байт (при base64url и ECC M это оставляет запас до предела 2331).
    Дополнительно генератор QR проверяет, что код построился и имеет не более
    177 модулей; если нет — `qr_too_large`. Тесты проверяют границу по
    фактическому генератору, а не по приблизительной оценке.
12. **Kaspersky TXT.** Ориентир — реальный формат экспорта Kaspersky PM:
    секции `Websites`, `Applications`, `Notes`, поля `Website name`,
    `Website URL`, `Login name`, `Login`, `Password`, `Comment`,
    `Application`, `Name`, `Text`, разделители `---`, multiline-поля.
    Парсер best-effort: неизвестные секции/строки не теряются молча, а
    попадают в отчёт; для других вариантов остаётся CSV/generic mapping.

## 4. Модель данных

### 4.1. Запись

```ts
type Entry = {
  // существующие поля
  url: string;          // legacy-зеркало первой ссылки
  urls: string[];       // новая истина; может быть пустым
  // остальные поля без изменений
};
```

- `EntryDraft` получает `urls: string[]` вместо одиночного `url`.
- `entryUrls(entry)` возвращает `urls`, а для старых записей — `[url]`.
- При сохранении: `url = urls[0] ?? ""`, `urls = urls`.
- Rust `Entry` получает `#[serde(default)] urls: Vec<String>`.
- Старые хранилища читаются без миграции: пустой `urls` + непустой `url`.
- Поиск, карточка, детали, QR и импорт используют все ссылки.
- Ссылки не обязаны быть http(s): импорт сохраняет любые строки; открытие
  внешнего браузера разрешено только для http/https, как сейчас.

### 4.2. QR-карточка

```json
{
  "v": 1,
  "name": "GitHub",
  "login": "alex",
  "password": "secret",
  "urls": ["https://github.com"],
  "comment": "личный аккаунт",
  "totp": {
    "secret": "JBSWY3DPEHPK3PXP",
    "algorithm": "SHA1",
    "digits": 6,
    "period": 30,
    "issuer": "GitHub",
    "account": "alex"
  }
}
```

- Без пароля: `PMSQR1:` + base64url(zlib(JSON)).
- С паролем: `PMSQR1E:` + base64url(версия || Argon2id-параметры ||
  salt || nonce || XChaCha20-Poly1305 ciphertext).
- Argon2id для QR: 19 МиБ, t=2, p=1 — быстрее импорта, но всё ещё не
  тривиальный перебор. Параметры входят в payload.
- Лимит QR — итоговая строка `PMSQR1:`/`PMSQR1E:` не более 2300 байт при
  ECC M (версия не выше 40). Если не влезает, показываем ошибку и подсказываем
  сократить комментарий/число URL; молча обрезать данные нельзя.
- В QR нет вложений, SSH и служебных id.

### 4.3. Импортируемые данные

```ts
type ImportedBundle = {
  format: string;
  items: ImportedItem[];
  warnings: ImportWarning[];
};
type ImportedItem = {
  kind: "login" | "note" | "card" | "identity" | "ssh";
  name: string;
  login: string;
  password: string;
  urls: string[];
  comment: string;
  totp: TotpConfig | null;
  folder?: string;
  tags: string[];
  customFields: { label: string; value: string }[];
  passwordHistory: { value: string; changedAt?: string }[];
  attachments: ImportedAttachment[];
  sshKey?: ImportedSshKey;
};
```

- Карты, identity, заметки и произвольные поля импортируются как обычные
  записи PMS. Подписанные поля, папка и теги сохраняются в комментарии
  отдельными строками, чтобы не терять данные; пароль/логин/TOTP/URL
  остаются структурными.
- SSH-ключи из Bitwarden/1Password/KeePass создаются в разделе SSH.
- Passkeys и неизвестные типы не создаются, но попадают в отчёт.
- Вложения возвращаются из Rust по одному (base64) и сразу шифруются
  существующим `attachment_seal`; в записях остаются id.

### 4.4. Локальная настройка биометрии

Ключ `pms.biometric.<vaultId>` в localStorage: `{ enabled: boolean }`.
Сам ключ хранилища — только в Keychain, не в localStorage.

## 5. Биометрия macOS

### 5.1. Доступность

- `LAContext.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics)`.
- На текущем Mac модуля Touch ID физически нет: `available = false`, чекбокс
  выключен с пояснением. Успешный сценарий проверит пользователь на MacBook.
- Если биометрия включена, но стала недоступна/набор отпечатков изменился —
  показываем ошибку и требуем пароль; локальную запись ключа удаляем.

### 5.2. Хранение

- Keychain: generic password, service `dev.pms.vault.biometric`,
  account = `vaultId`, value = 32-байтовый vault key.
- Access control: `kSecAccessControlBiometryCurrentSet` +
  `kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly`.
- Ключ не попадает в JS, логи и vault-файл.
- Для одной попытки создаётся `LAContext` с
  `deviceOwnerAuthenticationWithBiometrics`; после успеха Keychain читается
  с этим контекстом. Пароль ОС как fallback в системном диалоге не
  используется.

### 5.3. Команды

- `biometric_status(vaultId) -> { available, enrolled, error? }`.
- `biometric_enroll(sessionId)` — один биометрический запрос, затем
  сохранение vault key в Keychain.
- `biometric_disable(vaultId)` — удаление Keychain-элемента.
- `biometric_unlock_once(fileB64, vaultId) -> UnlockedVault` — одна попытка;
  ошибки `biometric_failed`, `biometric_cancelled`, `biometric_unavailable`,
  `biometric_lockout`, `biometric_key_missing`.
- Внутренняя `unlock_vault_with_key(file, key)` — расшифровка payload без
  KDF-слота; используется только после успешной биометрии.

### 5.4. Поток

1. Пользователь разблокировал хранилище паролем.
2. В настройках ставит галочку «Всегда разблокировать при помощи биометрии».
3. Приложение просит отпечаток, записывает vault key в Keychain и локальный
   флаг.
4. При следующем запуске приложение открывает последнее хранилище, видит
   флаг и сразу делает одну попытку. При неудаче показывает «Осталось N
   попыток» и кнопки «Повторить»/«Ввести пароль». После 5-й — только пароль.
5. На экране блокировки, если биометрия включена, есть кнопка «Войти по
   отпечатку»; авто-запрос выполняется только при запуске, не при каждом
   ручном lock.

### 5.5. Безопасность и ограничения

- 5 попыток считает приложение; системный lockout тоже ведёт к паролю.
- Отмена = отказ, сразу пароль.
- Биометрия не синхронизируется и не входит в `.pmsvault`.
- Если Keychain-элемент пропал, чекбокс выключается, данные не теряются.
- FFI Keychain access control и LAContext — отдельный слой с unit-тестами
  логики и ручной проверкой на MacBook.

## 6. QR и TOTP

### 6.1. Сканер

- Общий компонент `QrScannerDialog` с двумя вкладками: «Изображение» и
  «Камера». Пользователь сам выбирает способ.
- Изображение: файл → canvas → `jsQR`.
- Камера: `getUserMedia({ video: { facingMode: "environment" } })` + кадры
  в `jsQR`; остановка потока при закрытии. Если камеры нет/доступ запрещён,
  вкладка показывает объяснение и остаётся изображение.
- `NSCameraUsageDescription` добавляется в macOS Info.plist; камера
  запрашивается только по действию пользователя.
- Сканирование TOTP: принимаем `otpauth://totp/...` и base32; результат
  проходит существующий `totp_inspect` и заполняет TOTP в редакторе.
- Показ QR для существующего TOTP: кнопка в деталях TOTP/записи, тот же
  генератор, `otpauth://`.
- Ошибки: не QR, не TOTP, нет камеры, доступ запрещён, пустой кадр.

### 6.2. Передача карточки

- В деталях записи — действие «Поделиться по QR».
- Диалог:
  - переключатель «Без пароля — проще» (по умолчанию) / «С паролем —
    безопаснее»;
  - предупреждение, что без пароля QR содержит секреты;
  - кнопка «Создать QR»;
  - QR-код, размер payload, кнопка «Сохранить PNG»;
  - если не влезает — ошибка с причиной.
- Принимающее устройство: «Добавить по QR» на экране записей или в
  настройках. Сканирует, при необходимости спрашивает пароль, показывает
  предпросмотр (имя, логин, URL, TOTP) и создаёт копию записи.
- Дубликат: если есть запись с теми же name/login/password/urls, спрашиваем
  «Создать копию / Пропустить». По умолчанию — создать копию.
- Вложения и SSH не передаются и об этом сказано в диалоге.

### 6.3. Формат и криптография

- Кодирование/декодирование payload — Rust-команды `share_encode` и
  `share_decode`, чтобы не дублировать Argon2id/XChaCha20 в JS.
- Без пароля — сжатый JSON. С паролем — сжатый JSON, зашифрованный
  Argon2id + XChaCha20-Poly1305.
- Обе стороны проверяют версию, длину, MAC/тег и размеры; неизвестная
  версия — понятная ошибка.
- Пароль QR не сохраняется в vault.

## 7. Импорт из других менеджеров

### 7.1. Общий поток

1. Настройки → «Импорт из другого менеджера».
2. Выбор файла. Формат определяется автоматически; для неизвестного CSV
   показываем сопоставление колонок.
3. Если формат зашифрован — запрос пароля (и key-файла для KDBX).
4. Разбор в Rust, предпросмотр: формат, количество записей/вложений/SSH,
   предупреждения, список пропущенного.
5. Импорт в текущее хранилище с опцией «Пропускать точные дубликаты»
   (по умолчанию включена) или «Импортировать всё».
6. Итог: создано записей, вложений, SSH-ключей, пропущено, ошибки.
   Пользователь может перейти к записям.

Импорт всегда в открытое хранилище. Если хранилище не открыто, настройки
недоступны; сначала обычный вход.

### 7.2. Форматы

| Источник | Форматы | Что переносим |
|---|---|---|
| Bitwarden | JSON, CSV, ZIP с attachments, encrypted JSON | логины, заметки, карты, identity, TOTP, несколько URL, кастомные поля, вложения, SSH-ключи, история паролей |
| 1Password | 1PUX, CSV | логины, заметки, карты, identity, TOTP, URL, вложения/Document items, кастомные поля, история |
| KeePass | KDBX (пароль + optional keyfile), XML, CSV | записи, группы→комментарий, URL, notes, TOTP, вложения, custom fields |
| KeePassX | KDBX, CSV | то же, что KeePass |
| Kaspersky PM | TXT, CSV | websites/applications/notes; поля, логин, пароль, URL, комментарий |
| Браузеры | Chrome/Chromium/Edge/Brave CSV, Firefox CSV, Safari CSV | name, url, username, password, notes, OTPAuth |
| Прочие | generic CSV с ручным mapping | name, url(s), login, password, notes, TOTP |

Дополнительно поддерживаем известные CSV-заголовки 1Password, LastPass,
Dashlane, NordPass, Enpass; если заголовки не распознаны — mapping UI.

### 7.3. Bitwarden

- JSON: `folders`, `items`, `type` 1/2/3/4/5; `login.uris` → все URL;
  `login.totp` → TOTP; `fields` → custom fields; `passwordHistory`.
- ZIP: `data.json` + `attachments/<itemId>/...`; привязка по attachment id/
  имени; битые/отсутствующие файлы → предупреждение.
- Encrypted JSON password-protected:
  - PBKDF2-SHA256 (kdfType 0) и Argon2id (kdfType 1);
  - HKDF-SHA256 stretch (`enc`/`mac`), AES-256-CBC + HMAC-SHA256;
  - проверка `encKeyValidation` и MAC; ошибка пароля без утечки деталей.
- Account-restricted JSON: не поддерживаем, объясняем сделать
  password-protected экспорт.
- CSV: логины/заметки, поля, TOTP; без attachments/карт (как в экспорте).

### 7.4. 1Password 1PUX

- ZIP: `export.data` + `files/`; JSON-дерево accounts → vaults → items.
- `overview.urls` и `overview.url` → urls; `details.loginFields` designation
  username/password; `notesPlain`; `sections.fields`; TOTP field; attachments
  через `documentAttributes.documentId`/`fileName`; Document items — запись
  с вложением.
- `passwordHistory` сохраняем в историю PMS.
- 1Password CSV — через общий CSV-парсер.

### 7.5. KeePass

- KDBX: crate `keepass`; пароль + optional keyfile; KDBX3/KDBX4.
- Обход групп и записей; стандартные поля Title/UserName/Password/URL/Notes;
  custom fields и attachments; TOTP из `otp`/`TOTP Seed`/`TOTP Settings`.
- Группа и теги сохраняются в комментарии.
- KeePass XML — тот же маппинг без расшифровки.
- KeePassX CSV — общий CSV.

### 7.6. Kaspersky

- TXT: секции `Websites`, `Applications`, `Notes`; поля из экспортного
  формата; multiline comments; пустые записи пропускаются с предупреждением.
- CSV: если есть заголовки, общий CSV-парсер; если это тот же TXT-формат с
  разделителями — сначала TXT-парсер.
- Карты/документы Kaspersky в экспорт не попадают; импортируем то, что есть,
  без выдумывания.

### 7.7. Вложения и ограничения

- Вложения читаются из ZIP/KDBX/XML/1PUX; каждое проверяется по размеру,
  имени и MIME; path traversal запрещён.
- Лимит одного вложения — `maxAttachmentMb` (50/100), общий лимит импорта —
  500 МБ; превышение → предупреждение и пропуск, остальное импортируется.
- Вложения шифруются существующим форматом `.pmsa`; имена и MIME остаются
  в зашифрованных метаданных.
- Ошибка одного вложения не отменяет импорт остальных.
- После импорта исходный файл не копируется в vault; UI напоминает удалить
  plaintext-экспорт.

### 7.8. Дубликаты

- Точный дубликат: нормализованные name + login + password + urls.
- По умолчанию пропускается; в отчёте показывается количество.
- «Импортировать всё» создаёт копии.
- Слияние/замена существующих записей в этом этапе не делаем; это отдельная
  операция, чтобы не потерять данные.

## 8. Экспорт/импорт PMS

### 8.1. `.pmsvault`

ZIP:

```
manifest.json
pms.vault
attachments/<id>.pmsa
```

`manifest.json`:

```json
{
  "format": "pmsvault",
  "version": 1,
  "vaultId": "…",
  "createdAt": "…",
  "exportedAt": "…",
  "files": [{ "path": "pms.vault", "size": 1234, "sha256": "…" }]
}
```

- `pms.vault` и `.pmsa` уже зашифрованы; manifest не содержит секретов.
- Экспорт доступен только при разблокированном хранилище; текущее состояние
  сохраняется перед экспортом.
- Не включаем `backups/` и временные файлы.
- Импорт: выбрать `.pmsvault`, выбрать новую/пустую папку; проверяются
  manifest, версия, checksums, path traversal, zip-bomb (распакованный объём
  ≤ 2× архива или 2 ГиБ), затем папка открывается как обычное хранилище.
  Точка входа есть и на экране приветствия: «Импортировать .pmsvault» —
  чтобы восстановление работало без временного хранилища.
- Перезапись существующего `pms.vault` запрещена; ошибка объясняет, что
  нужно выбрать другую папку.
- Android SAF-версия появится на следующем этапе; интерфейс команд сохраняет
  разделение «прочитать архив / распаковать в папку».

## 9. UI

- `SettingsView` вместо заглушки:
  - «Безопасность»: чекбокс «Всегда разблокировать при помощи биометрии»,
    статус Touch ID, кнопка «Забыть отпечаток».
  - «Импорт и экспорт»: импорт из другого менеджера, экспорт `.pmsvault`,
    импорт `.pmsvault`.
  - «О программе».
- `EntryEditor`: список URL с `+`/удалением; поле TOTP с кнопкой QR.
- `EntryDetail`: все URL, кнопка «Поделиться по QR», TOTP QR.
- `UnlockScreen`: кнопка «Войти по отпечатку», сообщение о попытках,
  откат к паролю.
- `QrScannerDialog`, `ShareCardDialog`, `ImportWizard`, `ExportDialog`.
- `WelcomeScreen`: кнопка «Импортировать .pmsvault» с выбором архива и
  папки назначения; после распаковки — обычный экран разблокировки.
- Сохраняем существующий визуальный язык: тёмный «сейф», мятный акцент,
  Lucide, без новых декоративных паттернов.

## 10. Ошибки и безопасность

- Биометрия: `biometric_unavailable`, `biometric_not_enrolled`,
  `biometric_failed`, `biometric_cancelled`, `biometric_lockout`,
  `biometric_key_missing`.
- QR: `qr_too_large`, `qr_invalid`, `qr_unsupported_version`,
  `qr_password_required`, `qr_password_invalid`, `qr_not_totp`.
- Импорт: `import_unsupported`, `import_encrypted_required`,
  `import_password_invalid`, `import_corrupt`, `import_zip_bomb`,
  `import_path_traversal`, `import_too_large`.
- Приватные ключи, пароли и TOTP-секреты не логируются. Ошибки паролей
  общие, без подсказок.
- QR без пароля явно предупреждает, что секреты видны сканирующему.
- Импорт plaintext-файлов — явное действие; после импорта предложение
  удалить файл.
- ZIP-импорт защищён от path traversal, абсолютных путей, symlink-выходов,
  zip-bomb и перезаписи.

## 11. Тестирование

### Rust

- `Entry.urls` roundtrip и миграция legacy `url`.
- `share_encode`/`share_decode`: plain, encrypted, wrong password, version,
  too large, TOTP fields, multiple URLs.
- Bitwarden: plain JSON, CSV, ZIP attachments, encrypted PBKDF2, encrypted
  Argon2id, wrong password, account-restricted error.
- 1PUX: items, multiple URLs, TOTP, attachments, Document item.
- KeePass: KDBX fixture with password (and keyfile if support), XML, attachments.
- Kaspersky TXT, browser CSV, generic CSV mapping.
- Import duplicate policy, limits, traversal, corrupt files.
- PMS archive: manifest/checksum/traversal/zip-bomb/refuse overwrite.
- `unlock_vault_with_key`; biometric state machine with mock provider.

### Frontend

- URL rows add/remove; search matches all URLs; legacy entry.
- Settings checkbox states and auto prompt; 5-attempt countdown/fallback.
- QR scanner image/camera fallbacks; share dialog default no password;
  encrypted share import.
- TOTP scan fills config; invalid QR errors.
- Import wizard preview/summary/duplicates; export dialogs.
- Store/API wiring and error text.

### E2E (mock Tauri)

- Biometric status unavailable/available; 5 failures → password; cancel.
- QR card share → import with and without password; QR capacity boundary.
- TOTP QR image import.
- Multiple URL entry save/search.
- Import mock bundle with attachment/SSH/card; duplicate skip.
- PMS export/import mock flow, including import from the welcome screen.
- Existing suites still pass.

### Manual

- macOS Touch ID on MacBook (user later), including enroll, auto prompt,
  cancel, 5 failures, changed fingerprints.
- Camera QR if a camera is available; otherwise image path.
- Real exports from Bitwarden/1Password/KeePass/Kaspersky/browsers.
- `.pmsvault` on another folder; wrong archive/truncated; no overwrite.
- Visual screenshots of settings, QR dialogs, import wizard, URL editor.

## 12. Этапы реализации

1. **Модель URL + Settings shell.** `Entry.urls`, draft/editor/detail/search,
   legacy migration, tests.
2. **Биометрия macOS.** Core key storage, commands, unlock flow, settings,
   5 attempts, auto-start, tests; manual check later.
3. **QR/TOTP.** Scanner, QR generation/parse, share dialog, TOTP scan/display,
   camera permission, tests.
4. **Импорт/экспорт.** Normalized import, format parsers, encrypted formats,
   attachments/SSH, wizard/report; PMS archive; tests.
5. **Проверки и сборка.** typecheck/test/lint/e2e, cargo test/clippy,
   `pnpm tauri build`, visual check, README.

## 13. Риски

- На текущем Mac нет Touch ID: успешный путь нельзя проверить до MacBook.
  Поэтому обязательны mock-тесты состояния и честный disabled-UI.
- WKWebView `getUserMedia` на macOS может требовать дополнительных
  entitlements/UI-delegate. Если камера не заработает — изображение остаётся
  полноценным путём; камера не блокирует этап.
- Keychain access control + LAContext FFI — самая рискованная часть
  биометрии; при проблемах оставляем строгую проверку через Keychain и
  explic- fallback, но не храним ключ в открытом виде.
- KeePass crate и KDBX4.1/Argon2-варианты: нужны фикстуры и проверка
  реальных баз.
- Bitwarden account-restricted JSON нерасшифровываем by design.
- Очень большие импорты ограничены памятью IPC; лимиты и предупреждения
  обязательны.
- QR-ёмкость: большие комментарии/много URL могут не влезть; показываем
  ошибку, не обрезаем.
