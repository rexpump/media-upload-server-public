# Deployment Guide

## Building for Production

### Prerequisites

- Rust 1.75+ (for edition 2021 features)
- System libraries: OpenSSL, pkg-config

### Build

```bash
# Build optimized release binary
cargo build --release

# Binary location
./target/release/media-upload-server
```

The release build is optimized with:
- LTO (Link Time Optimization)
- Single codegen unit
- Stripped debug symbols
- ~5-10 MB binary size

## Deployment Options

### 1. Standalone Binary

```bash
# Copy binary and config
cp target/release/media-upload-server /opt/media-server/
cp config.toml /opt/media-server/

# Create data directory
mkdir -p /var/lib/media-server

# Run
cd /opt/media-server
./media-upload-server
```

### 2. Systemd Service (Debian/Ubuntu)

Полная пошаговая инструкция для Debian 11/12 и Ubuntu 22.04+.

#### Шаг 1: Создание системного пользователя

```bash
# Создать пользователя без shell и home по умолчанию
sudo useradd --system --shell /usr/sbin/nologin --create-home \
    --home-dir /home/media-upload-server media-upload-server

# Проверить
id media-upload-server
# uid=xxx(media-upload-server) gid=xxx(media-upload-server) groups=xxx(media-upload-server)
```

> Да, имена пользователей с дефисом (`media-upload-server`) разрешены в Linux.

#### Шаг 2: Создание директорий

```bash
# Директория для бинарника и конфига
sudo mkdir -p /opt/media-upload-server

# Директория для данных (в home пользователя)
sudo mkdir -p /home/media-upload-server/data

# Установить владельца
sudo chown -R media-upload-server:media-upload-server /home/media-upload-server
sudo chown -R root:root /opt/media-upload-server
```

**Структура после установки:**
```
/opt/media-upload-server/
├── media-upload-server     # бинарник (root:root, 755)
└── config.toml             # конфиг (root:root, 644)

/home/media-upload-server/
└── data/                   # данные (media-upload-server:media-upload-server, 755)
    ├── originals/          # создаётся автоматически
    ├── optimized/          # создаётся автоматически
    ├── temp/               # создаётся автоматически
    └── rocksdb/            # создаётся автоматически
```

#### Шаг 3: Копирование файлов

```bash
# Скомпилировать (на сервере или локально)
cargo build --release

# Копировать бинарник
sudo cp target/release/media-upload-server /opt/media-upload-server/
sudo chmod 755 /opt/media-upload-server/media-upload-server

# Копировать и настроить конфиг
sudo cp config.toml /opt/media-upload-server/config.toml
sudo nano /opt/media-upload-server/config.toml
```

#### Шаг 4: Настройка конфига

Отредактировать `/opt/media-upload-server/config.toml`:

```toml
[server]
host = "127.0.0.1"          # Только localhost (за nginx)
port = 3000
admin_host = "127.0.0.1"
admin_port = 3001
base_url = "https://media.example.com"  # Ваш домен
request_timeout = 300
max_connections = 1000
cache_max_age = 31536000
cleanup_interval_seconds = 300

[storage]
data_dir = "/home/media-upload-server/data"  # Путь к данным
originals_dir = "originals"
optimized_dir = "optimized"
temp_dir = "temp"

[upload]
max_simple_upload_size = 52428800       # 50 MB
max_chunked_upload_size = 524288000     # 500 MB
chunk_size = 5242880                    # 5 MB
allowed_image_types = ["image/jpeg", "image/png", "image/gif", "image/webp"]
allowed_video_types = []
upload_session_timeout = 3600

[processing]
output_format = "webp"
output_quality = 85
max_image_dimension = 4096
keep_originals = true
strip_exif = true

[rate_limit]
enabled = true
requests_per_window = 100
window_seconds = 60
uploads_per_window = 20

[logging]
level = "info"
format = "json"             # JSON для production
file = ""

[auth]
enabled = true
api_keys = ["ваш-секретный-ключ-здесь"]  # Сгенерировать: openssl rand -hex 32
protected_paths = ["/api/upload"]
public_paths = ["/health", "/m/"]
```

#### Шаг 5: Создание systemd unit

Создать `/etc/systemd/system/media-upload-server.service`:

```ini
[Unit]
Description=Media Upload Server
Documentation=https://github.com/yourname/media-upload-server
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=media-upload-server
Group=media-upload-server

# Рабочая директория
WorkingDirectory=/opt/media-upload-server

# Запуск
ExecStart=/opt/media-upload-server/media-upload-server

# Перезапуск при падении
Restart=on-failure
RestartSec=5
StartLimitIntervalSec=60
StartLimitBurst=3

# Безопасность
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=/home/media-upload-server/data
PrivateTmp=true
PrivateDevices=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
RestrictNamespaces=true
RestrictRealtime=true
RestrictSUIDSGID=true
LockPersonality=true

# Ресурсы
LimitNOFILE=65535
MemoryMax=2G
CPUQuota=200%

# Логирование
StandardOutput=journal
StandardError=journal
SyslogIdentifier=media-upload-server

[Install]
WantedBy=multi-user.target
```

#### Шаг 6: Запуск сервиса

```bash
# Перечитать конфигурацию systemd
sudo systemctl daemon-reload

# Включить автозапуск
sudo systemctl enable media-upload-server

# Запустить
sudo systemctl start media-upload-server

# Проверить статус
sudo systemctl status media-upload-server

# Смотреть логи
sudo journalctl -u media-upload-server -f

# Перезапустить после изменения конфига
sudo systemctl restart media-upload-server
```

#### Шаг 7: Проверка работы

```bash
# Health check
curl http://127.0.0.1:3000/health/live
# {"status":"ok"}

# Stats
curl http://127.0.0.1:3000/health/stats
# {"total_media":0,"storage_bytes":0,...}

# Тест загрузки (с API ключом из конфига)
curl -X POST http://127.0.0.1:3000/api/upload \
  -H "X-API-Key: ваш-секретный-ключ-здесь" \
  -F "file=@test.jpg"
```

#### Шаг 8: Настройка Nginx (reverse proxy)

```bash
sudo apt install nginx
sudo nano /etc/nginx/sites-available/media-upload-server
```

```nginx
server {
    listen 80;
    server_name media.example.com;
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name media.example.com;

    ssl_certificate /etc/letsencrypt/live/media.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/media.example.com/privkey.pem;

    # Лимит загрузки
    client_max_body_size 100M;

    # Таймауты для chunked upload
    proxy_connect_timeout 60s;
    proxy_send_timeout 300s;
    proxy_read_timeout 300s;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Connection "";
        proxy_request_buffering off;
    }
}
```

```bash
# Активировать
sudo ln -s /etc/nginx/sites-available/media-upload-server /etc/nginx/sites-enabled/

# Проверить конфиг
sudo nginx -t

# Перезапустить
sudo systemctl reload nginx
```

#### Полезные команды

```bash
# Статус сервиса
sudo systemctl status media-upload-server

# Логи в реальном времени
sudo journalctl -u media-upload-server -f

# Логи за последний час
sudo journalctl -u media-upload-server --since "1 hour ago"

# Перезапуск
sudo systemctl restart media-upload-server

# Остановка
sudo systemctl stop media-upload-server

# Проверить использование диска
du -sh /home/media-upload-server/data/*

# Проверить открытые файлы процесса
sudo lsof -p $(pgrep media-upload-server) | wc -l
```

#### Генерация API ключа

```bash
# Сгенерировать безопасный ключ
openssl rand -hex 32
# Например: a1b2c3d4e5f6...

# Или через /dev/urandom
cat /dev/urandom | tr -dc 'a-zA-Z0-9' | fold -w 64 | head -n 1
```

### 3. Docker

Create `Dockerfile`:

```dockerfile
# Build stage
FROM rust:1.75-slim as builder

WORKDIR /app
COPY . .

RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/media-upload-server /usr/local/bin/
COPY config.toml /etc/media-server/

WORKDIR /app
VOLUME /data

ENV RUST_LOG=info

EXPOSE 3000 3001

CMD ["media-upload-server"]
```

```bash
# Build
docker build -t media-server .

# Run
docker run -d \
  --name media-server \
  -p 3000:3000 \
  -v /path/to/data:/data \
  -v /path/to/config.toml:/etc/media-server/config.toml \
  media-server
```

### 4. Docker Compose

```yaml
version: '3.8'

services:
  media-server:
    build: .
    ports:
      - "3000:3000"
    volumes:
      - media-data:/data
      - ./config.toml:/etc/media-server/config.toml:ro
    environment:
      - RUST_LOG=info
    restart: unless-stopped
    deploy:
      resources:
        limits:
          memory: 2G
          cpus: '2'

volumes:
  media-data:
```

## Reverse Proxy Setup

### Nginx

```nginx
upstream media_server {
    server 127.0.0.1:3000;
    keepalive 32;
}

server {
    listen 80;
    server_name media.example.com;
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name media.example.com;

    ssl_certificate /etc/letsencrypt/live/media.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/media.example.com/privkey.pem;

    # Upload size limit
    client_max_body_size 100M;

    # Timeouts for chunked uploads
    proxy_connect_timeout 60s;
    proxy_send_timeout 300s;
    proxy_read_timeout 300s;

    location / {
        proxy_pass http://media_server;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Connection "";

        # Buffering for uploads
        proxy_request_buffering off;
    }

    # Cache static media
    location /m/ {
        proxy_pass http://media_server;
        proxy_cache_valid 200 365d;
        add_header X-Cache-Status $upstream_cache_status;
    }
}
```

### Caddy

```caddyfile
media.example.com {
    encode zstd gzip

    # CORS для upload с фронтенда
    @allowed_origin header_regexp Origin ^https?://(example\.com|www\.example\.com|localhost(:\d+)?|127\.0\.0\.1(:\d+)?)$

    header @allowed_origin {
        Access-Control-Allow-Origin "{http.request.header.Origin}"
        Access-Control-Allow-Methods "GET, POST, PATCH, OPTIONS"
        Access-Control-Allow-Headers "Content-Type, Content-Range, X-API-Key, Authorization"
        Access-Control-Expose-Headers "ETag, Content-Length"
    }

    # Preflight запросы
    @preflight method OPTIONS
    respond @preflight "" 204

    # Кэширование для медиа файлов
    @media path /m/*
    header @media Cache-Control "public, max-age=31536000, immutable"

    # Лимит размера upload (500MB для chunked)
    request_body {
        max_size 500MB
    }

    reverse_proxy 127.0.0.1:3000 {
        # Таймауты для больших загрузок
        transport http {
            read_timeout 300s
            write_timeout 300s
        }
    }
}
```

**Параметры CORS:**
- Замените `example\.com` на ваши домены
- `PATCH` нужен для chunked uploads
- `X-API-Key` для авторизации (если включена)

## Monitoring

### Health Checks

```bash
# Liveness (is server running?)
curl http://localhost:3000/health/live

# Readiness (can accept requests?)
curl http://localhost:3000/health/ready

# Stats
curl http://localhost:3000/health/stats
```

### Prometheus Metrics (TODO)

Future version will expose `/metrics` endpoint.

### Log Aggregation

With JSON logging enabled:

```bash
# View logs
journalctl -u media-server -o json | jq

# Example with Loki/Grafana
# Configure promtail to scrape journald
```

## Backup & Recovery

### Data to Backup

1. **RocksDB Database** — `/home/media-upload-server/data/rocksdb/`
2. **Optimized Files** — `/home/media-upload-server/data/optimized/`
3. **Original Files** — `/home/media-upload-server/data/originals/` (если `keep_originals = true`)

### Backup Script

Создать `/opt/media-upload-server/backup.sh`:

```bash
#!/bin/bash
set -e

BACKUP_DIR=/backup/media-upload-server
DATA_DIR=/home/media-upload-server/data
DATE=$(date +%Y%m%d_%H%M%S)

echo "Starting backup: $DATE"

# Создать директорию бекапа
mkdir -p $BACKUP_DIR/$DATE

# Бекап RocksDB
# RocksDB безопасен для копирования на лету (использует WAL)
cp -r $DATA_DIR/rocksdb $BACKUP_DIR/$DATE/rocksdb
echo "RocksDB backed up"

# Бекап файлов (инкрементальный через hardlinks)
rsync -av --link-dest=$BACKUP_DIR/latest/originals \
    $DATA_DIR/originals/ \
    $BACKUP_DIR/$DATE/originals/

rsync -av --link-dest=$BACKUP_DIR/latest/optimized \
    $DATA_DIR/optimized/ \
    $BACKUP_DIR/$DATE/optimized/

echo "Files backed up"

# Обновить симлинк на последний бекап
ln -sfn $BACKUP_DIR/$DATE $BACKUP_DIR/latest

# Удалить бекапы старше 7 дней
find $BACKUP_DIR -maxdepth 1 -type d -mtime +7 -exec rm -rf {} \;

echo "Backup completed: $BACKUP_DIR/$DATE"
```

```bash
# Сделать исполняемым
sudo chmod +x /opt/media-upload-server/backup.sh

# Добавить в cron (ежедневно в 3:00)
sudo crontab -e
# 0 3 * * * /opt/media-upload-server/backup.sh >> /var/log/media-backup.log 2>&1
```

### Recovery

```bash
# Остановить сервер
sudo systemctl stop media-upload-server

# Восстановить RocksDB
sudo rm -rf /home/media-upload-server/data/rocksdb
sudo cp -r /backup/media-upload-server/latest/rocksdb /home/media-upload-server/data/

# Восстановить файлы
sudo rsync -av /backup/media-upload-server/latest/originals/ /home/media-upload-server/data/originals/
sudo rsync -av /backup/media-upload-server/latest/optimized/ /home/media-upload-server/data/optimized/

# Восстановить права
sudo chown -R media-upload-server:media-upload-server /home/media-upload-server/data

# Запустить сервер
sudo systemctl start media-upload-server
```

## Performance Tuning

### System Limits

```bash
# /etc/security/limits.conf
media-upload-server soft nofile 65535
media-upload-server hard nofile 65535

# /etc/sysctl.conf
net.core.somaxconn = 65535
net.ipv4.tcp_max_syn_backlog = 65535

# Применить sysctl
sudo sysctl -p
```

### Storage

- Use SSD for data directory
- Consider separate disks for originals vs optimized
- Enable filesystem journaling (ext4, xfs)

### Memory

- Default memory usage: ~50-100 MB baseline
- Add ~1 MB per concurrent upload
- Configure `MemoryMax` in systemd accordingly

