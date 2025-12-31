# Configuration Guide

## Configuration Files

The server looks for configuration in the following order:

1. `config.local.toml` - Local overrides (gitignored)
2. `config.toml` - Main configuration

## Configuration Reference

### Server Settings

```toml
[server]
# Host to bind the public API
# Use "0.0.0.0" to accept connections from any interface
host = "0.0.0.0"

# Port for the public API
port = 3000

# Host for admin API (should always be localhost for security)
admin_host = "127.0.0.1"

# Port for admin API
admin_port = 3001

# Base URL for generating media URLs
# This should be your public domain in production
base_url = "https://media.example.com"

# Request timeout in seconds
request_timeout = 300

# Maximum concurrent connections
max_connections = 1000

# Cache-Control max-age in seconds (default: 31536000 = 1 year)
cache_max_age = 31536000

# Cleanup interval for expired upload sessions in seconds
cleanup_interval_seconds = 300
```

### Storage Settings

```toml
[storage]
# Base directory for all data
# Can be absolute or relative to working directory
data_dir = "./data"

# Subdirectory for original files
originals_dir = "originals"

# Subdirectory for optimized (WebP) files
optimized_dir = "optimized"

# Subdirectory for temporary upload files
temp_dir = "temp"

# Number of directory nesting levels for file storage (0-4)
# Each level uses 2 hex characters from UUID to create subdirectories.
# This prevents filesystem degradation with many files in one directory.
#   0 = flat:       originals/{uuid}.jpg
#   1 = 256 dirs:   originals/ab/{uuid}.jpg
#   2 = 65K dirs:   originals/ab/cd/{uuid}.jpg (recommended)
#   3 = 16M dirs:   originals/ab/cd/ef/{uuid}.jpg
#   4 = 4B dirs:    originals/ab/cd/ef/gh/{uuid}.jpg
directory_levels = 2
```

**Структура данных (с directory_levels = 2):**

```
data_dir/
├── originals/           # Оригинальные файлы
│   └── 55/0e/           # Подпапки из первых 4 символов UUID
│       └── 550e8400-e29b-41d4-a716-446655440000.jpg
├── optimized/           # Оптимизированные файлы
│   └── 55/0e/
│       └── 550e8400-e29b-41d4-a716-446655440000.webp
├── temp/                # Временные файлы (chunked uploads)
│   └── {session-id}/
│       └── upload
└── rocksdb/             # База данных RocksDB
```

**Почему подпапки важны:**
- ext4 начинает тормозить при >10,000 файлов в папке
- С `directory_levels = 2` создаётся до 65,536 подпапок
- Даже при миллионах файлов — в среднем <100 файлов на папку

**RocksDB** используется для хранения метаданных:
- Отличная защита от крашей (LSM-tree + WAL)
- Высокая скорость записи
- Атомарные batch-операции

### Upload Settings

```toml
[upload]
# Maximum file size for simple (single-request) upload
# Default: 50 MB
max_simple_upload_size = 52428800

# Maximum file size for chunked upload
# Default: 500 MB
max_chunked_upload_size = 524288000

# Chunk size for chunked uploads
# Default: 5 MB
chunk_size = 5242880

# Allowed image MIME types
allowed_image_types = [
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp"
]

# Allowed video MIME types (for future use)
allowed_video_types = [
    "video/mp4",
    "video/webm",
    "video/quicktime"
]

# Upload session timeout in seconds
# Sessions older than this are cleaned up
upload_session_timeout = 3600
```

### Image Processing Settings

```toml
[processing]
# Output format for optimized images: webp, jpeg, png
# WebP recommended for best compression
output_format = "webp"

# Quality for lossy formats (0-100)
# Higher = better quality, larger files
# Recommended: 80-90
output_quality = 85

# Maximum image dimension (width or height)
# Images larger than this will be resized
max_image_dimension = 4096

# Whether to keep original files
# Set to false to save disk space
keep_originals = true

# Whether to strip EXIF metadata
# Recommended for privacy
strip_exif = true
```

### Rate Limiting Settings

```toml
[rate_limit]
# Enable/disable rate limiting
enabled = true

# Maximum requests per window (any endpoint)
requests_per_window = 100

# Window duration in seconds
window_seconds = 60

# Maximum uploads per window (upload endpoints only)
uploads_per_window = 20
```

### Logging Settings

```toml
[logging]
# Log level: trace, debug, info, warn, error
level = "info"

# Log format: "pretty" or "json"
# Use "json" for production (log aggregation)
format = "pretty"

# Log to file (optional, leave empty to disable)
file = ""
```

### Authentication Settings

```toml
[auth]
# Enable API key authentication for uploads
enabled = true

# List of valid API keys
# Generate secure keys: openssl rand -hex 32
api_keys = [
    "your-secure-api-key-1",
    "your-secure-api-key-2"
]

# Paths that require authentication (empty = all paths except public)
protected_paths = ["/api/upload"]

# Paths that are always public (bypass auth even if enabled)
public_paths = ["/health", "/m/"]
```

**Генерация API ключей:**

```bash
# Linux/macOS
openssl rand -hex 32

# Пример вывода: a1b2c3d4e5f6...
```

## Environment Variables

You can override the log level with an environment variable:

```bash
RUST_LOG=debug ./media-upload-server
```

## Example Configurations

### Development

```toml
# config.local.toml

[server]
host = "127.0.0.1"
port = 3000
admin_port = 3001
base_url = "http://localhost:3000"

[storage]
data_dir = "./data-dev"
directory_levels = 2

[logging]
level = "debug"
format = "pretty"

[rate_limit]
enabled = false
```

### Production

```toml
# config.toml

[server]
host = "0.0.0.0"
port = 3000
admin_host = "127.0.0.1"
admin_port = 3001
base_url = "https://media.yoursite.com"
request_timeout = 60
max_connections = 10000

[storage]
data_dir = "/var/lib/media-server"
directory_levels = 2  # 65K подпапок для масштабирования

[upload]
max_simple_upload_size = 10485760      # 10 MB
max_chunked_upload_size = 104857600    # 100 MB

[processing]
output_format = "webp"
output_quality = 85
max_image_dimension = 2048
keep_originals = false  # Save disk space
strip_exif = true

[rate_limit]
enabled = true
requests_per_window = 60
window_seconds = 60
uploads_per_window = 10

[logging]
level = "info"
format = "json"
```

### High-Traffic

```toml
# config.toml

[server]
host = "0.0.0.0"
port = 3000
max_connections = 50000

[storage]
# Use SSD for best performance (RocksDB loves fast storage)
data_dir = "/mnt/ssd/media"
directory_levels = 3  # 16M подпапок для очень большого количества файлов

[upload]
max_simple_upload_size = 5242880   # 5 MB
max_chunked_upload_size = 52428800 # 50 MB
chunk_size = 1048576               # 1 MB chunks

[processing]
output_format = "webp"
output_quality = 80
max_image_dimension = 1920
keep_originals = false
strip_exif = true

[rate_limit]
enabled = true
requests_per_window = 30
uploads_per_window = 5

[logging]
level = "warn"
format = "json"
```

## Storage Sizing Guide

Estimate storage needs:

| Images/day | Avg Size (WebP) | Monthly Storage |
|------------|-----------------|-----------------|
| 1,000 | 100 KB | ~3 GB |
| 10,000 | 100 KB | ~30 GB |
| 100,000 | 100 KB | ~300 GB |

With `keep_originals = true`, roughly double these estimates.

---

## RexPump Configuration

The `[rexpump]` section enables the token metadata API for RexPump mempad tokens.

### Basic Configuration

```toml
[rexpump]
# Enable/disable the RexPump API
enabled = true

# Minimum seconds between updates for the same token
update_cooldown_seconds = 60

# Maximum age of signature timestamp (anti-replay protection)
signature_max_age_seconds = 300
```

### Network Configuration

Configure supported EVM networks for token ownership verification:

```toml
[rexpump.networks.zilliqa_mainnet]
name = "zilliqa_mainnet"
chain_id = 32769
rpc_url = "https://ssn.zilpay.io/api"
fallback_rpc_url = "https://api.zilliqa.com"

[rexpump.networks.zilliqa_testnet]
name = "zilliqa_testnet"
chain_id = 33101
rpc_url = "https://dev-api.zilliqa.com"
```

### Network Options

| Option | Description |
|--------|-------------|
| `name` | Network identifier (for logging) |
| `chain_id` | EVM chain ID |
| `rpc_url` | Primary JSON-RPC endpoint |
| `fallback_rpc_url` | Optional backup RPC (used if primary fails) |

### Security Considerations

1. **Signature Verification**: Uses EIP-191 personal_sign with timestamp
2. **Anti-Replay**: Signatures expire after `signature_max_age_seconds`
3. **Rate Limiting**: Per-token cooldown via `update_cooldown_seconds`
4. **Owner Verification**: Calls on-chain `creator()` function via RPC

### Production Tips

- Use reliable RPC providers with fallback
- Set appropriate cooldown to prevent spam
- Monitor RPC availability for token verification
```
