# Development Guide

## Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs))
- Git

## Getting Started

```bash
# Clone repository
git clone https://github.com/yourname/media-upload-server
cd media-upload-server

# Build
cargo build

# Run tests
cargo test

# Run server
cargo run
```

## Project Structure

```
media-upload-server/
├── Cargo.toml           # Dependencies and metadata
├── config.toml          # Default configuration
├── src/
│   ├── main.rs          # Entry point
│   ├── lib.rs           # Library exports, run()
│   ├── config.rs        # Configuration loading
│   ├── error.rs         # Error types and handling
│   ├── state.rs         # Application state
│   ├── handlers/        # HTTP request handlers
│   │   ├── mod.rs
│   │   ├── upload.rs    # Upload endpoints
│   │   ├── serve.rs     # Media serving
│   │   ├── admin.rs     # Admin endpoints
│   │   └── health.rs    # Health checks
│   ├── services/        # Business logic
│   │   ├── mod.rs
│   │   ├── storage.rs   # File operations
│   │   ├── database.rs  # RocksDB operations
│   │   └── image_processor.rs  # Image processing
│   ├── middleware/      # HTTP middleware
│   │   ├── mod.rs
│   │   ├── auth.rs      # API key authentication
│   │   └── rate_limit.rs # Rate limiting
│   └── models/          # Data structures
│       ├── mod.rs
│       ├── media.rs     # Media entity
│       └── upload_session.rs  # Upload session
├── tests/               # Integration tests
│   ├── common/
│   │   └── mod.rs       # TestServer, helpers
│   ├── upload_test.rs   # Upload API tests
│   ├── serve_test.rs    # Media serving tests
│   ├── chunked_upload_test.rs  # Chunked upload tests
│   └── admin_test.rs    # Admin API tests
└── docs/                # Documentation
```

## Code Style

The project follows standard Rust conventions:

- `rustfmt` for formatting
- `clippy` for linting
- Doc comments on all public items

```bash
# Format code
cargo fmt

# Run linter
cargo clippy -- -D warnings

# Check all
cargo fmt --check && cargo clippy -- -D warnings && cargo test
```

## Testing

### Запуск всех тестов

```bash
# Все тесты (unit + интеграционные)
cargo test

# Только unit-тесты
cargo test --lib

# Только интеграционные тесты
cargo test --tests

# Конкретный тест-файл
cargo test --test upload_test
cargo test --test serve_test
cargo test --test chunked_upload_test
cargo test --test admin_test

# Конкретный тест
cargo test test_simple_upload_png

# С выводом stdout
cargo test -- --nocapture
```

### Unit Tests (22 теста)

Расположены в каждом модуле рядом с кодом:

| Модуль | Тесты |
|--------|-------|
| `config` | Пути хранения, валидация типов |
| `error` | Статус-коды, категории ошибок |
| `models/media` | Типы медиа, имена файлов |
| `models/upload_session` | Статусы сессий, прогресс |
| `services/database` | CRUD операции RocksDB |
| `services/storage` | Сохранение/удаление файлов |
| `services/image_processor` | Определение MIME, форматы |
| `middleware/auth` | Авторизация API ключей |
| `middleware/rate_limit` | Rate limiting |

### Интеграционные тесты (21 тест)

Расположены в папке `tests/`:

```
tests/
├── common/
│   └── mod.rs              # TestServer, хелперы
├── upload_test.rs          # 6 тестов загрузки
├── serve_test.rs           # 5 тестов раздачи
├── chunked_upload_test.rs  # 6 тестов chunked upload
└── admin_test.rs           # 4 теста админ API
```

#### upload_test.rs — Загрузка файлов

| Тест | Описание |
|------|----------|
| `test_simple_upload_png` | Загрузка PNG, конвертация в WebP |
| `test_simple_upload_jpeg` | Загрузка JPEG |
| `test_upload_invalid_file_type` | Отклонение неподдерживаемых типов |
| `test_upload_empty_file` | Обработка пустых файлов |
| `test_upload_deduplication` | Дедупликация по хешу контента |
| `test_upload_with_auth_required` | Проверка API key авторизации |

#### serve_test.rs — Раздача медиа

| Тест | Описание |
|------|----------|
| `test_serve_uploaded_image` | Получение WebP, заголовки Cache-Control |
| `test_serve_original_image` | Получение оригинала (PNG) |
| `test_serve_nonexistent_image` | 404 для несуществующего ID |
| `test_serve_invalid_uuid` | 400 для невалидного UUID |
| `test_etag_caching` | ETag и 304 Not Modified |

#### chunked_upload_test.rs — Чанкованная загрузка

| Тест | Описание |
|------|----------|
| `test_chunked_upload_init` | Инициализация сессии |
| `test_chunked_upload_full_flow` | Полный цикл: init → chunk → complete |
| `test_chunked_upload_multiple_chunks` | Загрузка несколькими чанками |
| `test_chunked_upload_status` | Проверка статуса сессии |
| `test_chunked_upload_invalid_session` | 404 для несуществующей сессии |
| `test_chunked_upload_invalid_content_range` | Невалидный Content-Range |

#### admin_test.rs — Админ API

| Тест | Описание |
|------|----------|
| `test_admin_delete_media` | Удаление медиа |
| `test_admin_delete_nonexistent` | 404 при удалении несуществующего |
| `test_admin_stats` | Получение статистики |
| `test_admin_get_media_info` | Информация о медиа |

### Написание тестов

#### Unit тест

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        assert_eq!(2 + 2, 4);
    }
}
```

#### Интеграционный тест

```rust
// tests/my_test.rs
mod common;
use common::{create_test_png, TestServer};

#[tokio::test]
async fn test_my_feature() {
    let server = TestServer::start().await;
    let client = server.client();

    let response = client
        .get(server.url("/api/endpoint"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
}
```

### Manual Testing

```bash
# Simple upload
curl -X POST http://localhost:3000/api/upload \
  -F "file=@/path/to/image.jpg" | jq

# Chunked upload
# 1. Initialize
SESSION=$(curl -s -X POST http://localhost:3000/api/upload/init \
  -H "Content-Type: application/json" \
  -d '{"filename":"test.jpg","mime_type":"image/jpeg","total_size":1000}' | jq -r .id)

# 2. Upload chunk
curl -X PATCH "http://localhost:3000/api/upload/$SESSION/chunk" \
  -H "Content-Range: bytes 0-999/1000" \
  --data-binary @file.jpg

# 3. Complete
curl -X POST "http://localhost:3000/api/upload/$SESSION/complete"

# Get media
curl http://localhost:3000/m/{id} --output image.webp

# Admin: delete
curl -X DELETE http://localhost:3001/admin/media/{id}
```

## Adding Features

### New Endpoint

1. Create handler in `src/handlers/`
2. Add route in handler's `*_routes()` function
3. Update `src/handlers/mod.rs` exports
4. Add documentation in `docs/api.md`

Example:

```rust
// src/handlers/example.rs

use axum::{routing::get, Router, Json};
use crate::state::AppState;

async fn my_endpoint() -> Json<&'static str> {
    Json("Hello!")
}

pub fn example_routes() -> Router<AppState> {
    Router::new()
        .route("/example", get(my_endpoint))
}
```

### New Service

1. Create module in `src/services/`
2. Add to `src/services/mod.rs`
3. Initialize in `AppState::new()` if needed

### New Model

1. Create struct in `src/models/`
2. Add to `src/models/mod.rs`
3. Implement `Serialize`/`Deserialize` as needed

## Error Handling

Use the `AppError` type for all errors:

```rust
use crate::error::{AppError, Result};

fn my_function() -> Result<String> {
    if something_wrong {
        return Err(AppError::validation("Something is wrong"));
    }
    Ok("Success".to_string())
}
```

Error types map to HTTP status codes automatically:

- `AppError::validation()` → 400 Bad Request
- `AppError::not_found()` → 404 Not Found
- `AppError::internal()` → 500 Internal Server Error

## Logging

Use `tracing` macros:

```rust
use tracing::{info, debug, warn, error};

fn process() {
    debug!(key = "value", "Debug message");
    info!("Info message");
    warn!(error = %e, "Warning with error");
    error!("Error message");
}
```

## Performance Considerations

1. **Async everywhere** - Never block the async runtime
2. **Streaming** - Use streams for large files, not buffering
3. **Connection pooling** - Reuse database connections
4. **Caching** - Use appropriate cache headers

## Release Process

1. Update version in `Cargo.toml`
2. Update CHANGELOG.md
3. Run full test suite
4. Build release: `cargo build --release`
5. Tag release: `git tag v0.1.0`
6. Push: `git push --tags`

