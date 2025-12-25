# 📸 Media Upload Server

Высокопроизводительный сервер для загрузки и раздачи медиафайлов, написанный на Rust.

## ✨ Возможности

- **Загрузка изображений** — простая загрузка через multipart form
- **Chunked Upload** — загрузка больших файлов по частям с поддержкой докачки
- **Автоматическая оптимизация** — конвертация в WebP для уменьшения размера
- **Дедупликация** — одинаковые файлы хранятся только один раз
- **Admin API** — приватный API для модерации контента
- **Высокая производительность** — асинхронный I/O, минимальное потребление ресурсов

## 🚀 Быстрый старт

```bash
# Клонировать репозиторий
git clone https://github.com/yourname/media-upload-server
cd media-upload-server

# Запустить сервер
cargo run --release

# Загрузить изображение
curl -X POST http://localhost:3000/api/upload -F "file=@image.jpg"

# Получить изображение
curl http://localhost:3000/m/{id} --output image.webp
```

## 📋 API

### Загрузка

```bash
# Простая загрузка
POST /api/upload
Content-Type: multipart/form-data

# Chunked upload (для больших файлов)
POST /api/upload/init          # Инициализация
PATCH /api/upload/{id}/chunk   # Загрузка chunk'а
POST /api/upload/{id}/complete # Завершение
GET /api/upload/{id}/status    # Статус (для докачки)
```

### Получение медиа

```bash
GET /m/{id}          # WebP версия (оптимизированная)
GET /m/{id}/original # Оригинал
```

### Admin API (localhost:3001)

```bash
DELETE /admin/media/{id}  # Удаление
GET /admin/media/{id}     # Информация
POST /admin/cleanup       # Очистка просроченных сессий
```

## ⚙️ Конфигурация

```toml
# config.toml

[server]
host = "0.0.0.0"
port = 3000
admin_host = "127.0.0.1"
admin_port = 3001
base_url = "http://localhost:3000"

[storage]
data_dir = "./data"

[upload]
max_simple_upload_size = 52428800    # 50 MB
max_chunked_upload_size = 524288000  # 500 MB

[processing]
webp_quality = 85
max_image_dimension = 4096
keep_originals = true
strip_exif = true
```

Полная документация: [docs/configuration.md](./docs/configuration.md)

## 📁 Структура хранения

```
data/
├── originals/     # Оригинальные файлы
│   └── {uuid}.jpg
├── optimized/     # WebP версии
│   └── {uuid}.webp
├── temp/          # Временные файлы chunked upload
└── rocksdb/       # RocksDB база метаданных
```

## 🔒 Безопасность

- **Валидация по magic bytes** — не доверяем Content-Type заголовку
- **UUID для имён файлов** — защита от path traversal
- **Admin API только на localhost** — безопасная модерация
- **EXIF stripping** — удаление метаданных (GPS и т.д.)
- **Rate limiting** — защита от спама

## 📖 Документация

- [Архитектура](./docs/overview.md)
- [API Reference](./docs/api.md)
- [Конфигурация](./docs/configuration.md)
- [Деплой](./docs/deployment.md)
- [Разработка](./docs/development.md)

## 🛠️ Технологии

- **[Axum](https://github.com/tokio-rs/axum)** — Web framework
- **[Tokio](https://tokio.rs)** — Async runtime
- **[image](https://github.com/image-rs/image)** — Image processing
- **[RocksDB](https://rocksdb.org)** — Metadata storage (crash-safe)

## 📄 Лицензия

AGPL-3.0

