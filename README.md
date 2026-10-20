# PMS

Локальный менеджер паролей, TOTP-секретов и SSH-ключей. Хранилище — переносимая
зашифрованная папка, без сервера и телеметрии.

## Статус

В разработке. Первый срез: macOS Apple Silicon, создание/разблокировка
хранилища, записи, генераторы, история и авто-блокировка.

## Разработка

```bash
pnpm install
pnpm tauri dev
```

Проверки:

```bash
pnpm typecheck
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Документы:

- дизайн: `docs/superpowers/specs/2026-10-20-pms-design.md`
- план: `docs/superpowers/plans/2026-10-20-01-foundation-vault-entries.md`

## Безопасность

Никаких секретов в логах. Пароль-фраза и ключ-файл не хранятся. Потеря всех
способов входа означает потерю данных — восстановления нет.
