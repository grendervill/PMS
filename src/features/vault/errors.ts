const messages: Record<string, string> = {
  invalid_credentials:
    "Не удалось разблокировать. Проверьте пароль-фразу или ключ-файл.",
  invalid_totp: "Не удалось разобрать TOTP. Проверьте base32-секрет или otpauth:// ссылку.",
  invalid_ssh_name:
    "Имя SSH-файла может содержать только буквы, цифры, точку, дефис и подчёркивание.",
  ssh_passphrase_required: "У SSH-ключа есть парольная фраза. Введите её.",
  ssh_passphrase_invalid: "Неверная парольная фраза SSH-ключа.",
  ssh_key_invalid: "Не удалось прочитать SSH-ключ. Проверьте формат.",
  ssh_storage: "Не удалось получить доступ к папке SSH. Проверьте права и свободное место.",
  file_exists: "Файл с таким именем уже существует.",
  ssh_acl: "Не удалось выставить права доступа к SSH-файлам.",
  qr_password_required: "Этот QR защищён паролем. Введите пароль.",
  qr_password_invalid: "Неверный пароль QR-кода.",
  qr_invalid: "Не удалось прочитать QR-код. Проверьте изображение.",
  qr_too_large: "Данные не помещаются в QR-код. Сократите комментарий или число ссылок.",
  keyfile_passphrase_required: "У ключа есть парольная фраза. Введите её.",
  keyfile_passphrase_invalid: "Неверная парольная фраза ключа.",
  keyfile_public_key: "Это публичный ключ. Выберите приватный ключ-файл.",
  unsupported_keyfile:
    "Этот формат ключ-файла пока не поддерживается. Используйте OpenSSH или незашифрованный PEM/PKCS#8.",
  bad_magic:
    "Файл повреждён или это не хранилище PMS. Проверьте, что выбрана правильная папка.",
  truncated: "Файл хранилища обрезан или скопирован не полностью.",
  bad_header: "Заголовок хранилища повреждён.",
  bad_params: "В файле хранилища некорректные параметры шифрования.",
  bad_payload: "Данные хранилища повреждены.",
  unsupported_version: "Эта версия хранилища пока не поддерживается.",
  no_slots: "В файле нет способов входа.",
  too_many_slots: "В файле слишком много способов входа.",
  vault_id_mismatch: "Файл хранилища не совпадает с его заголовком.",
  serde: "Внутренние данные хранилища повреждены.",
  crypto: "Криптографическая ошибка. Файл повреждён.",
  session_not_found: "Сессия истекла. Разблокируйте хранилище снова.",
  storage:
    "Не удалось получить доступ к папке хранилища. Проверьте права, диск и свободное место.",
  file_too_large: "Файл слишком большой для этой операции.",
  empty_name: "Название хранилища не может быть пустым.",
  vault_exists: "В этой папке уже есть хранилище.",
  bad_base64: "Некорректные данные файла.",
  empty_secret: "Введите пароль-фразу или выберите ключ-файл.",
  last_unlock_method: "Нельзя удалить последний способ входа.",
  slot_not_found: "Способ входа не найден.",
  unknown: "Не удалось выполнить операцию.",
};

export function errorCode(error: unknown): string | null {
  if (error && typeof error === "object" && "code" in error) {
    const code = (error as { code?: unknown }).code;
    return typeof code === "string" ? code : null;
  }
  return null;
}

export function errorText(error: unknown): string {
  if (!error) return messages.unknown ?? "Не удалось выполнить операцию";

  const code = errorCode(error);
  if (code && messages[code]) return messages[code];

  const message =
    error instanceof Error
      ? error.message
      : error && typeof error === "object" && "message" in error
        ? String((error as { message?: unknown }).message)
        : "";

  if (message && /[А-Яа-яЁё]/.test(message)) return message;
  return messages.unknown ?? "Не удалось выполнить операцию";
}
