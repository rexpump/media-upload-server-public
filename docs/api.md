# API Reference

## Base URLs

- **Public API**: `http://localhost:3000`
- **Admin API**: `http://localhost:3001` (localhost only)

---

## Public API

### Upload Endpoints

#### Simple Upload

Upload a file in a single request.

```
POST /api/upload
Content-Type: multipart/form-data
```

**Request:**

```bash
curl -X POST http://localhost:3000/api/upload \
  -F "file=@image.jpg"
```

**Response (201 Created):**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "url": "http://localhost:3000/m/550e8400-e29b-41d4-a716-446655440000",
  "original_url": "http://localhost:3000/m/550e8400-e29b-41d4-a716-446655440000/original",
  "media_type": "image",
  "mime_type": "image/webp",
  "size": 45678,
  "width": 1920,
  "height": 1080
}
```

**Errors:**

| Status | Code | Description |
|--------|------|-------------|
| 400 | validation_error | Invalid request format |
| 413 | payload_too_large | File exceeds max size |
| 415 | unsupported_media_type | File type not allowed |
| 429 | rate_limit_exceeded | Too many requests |

---

#### Initialize Chunked Upload

Start a resumable upload session.

```
POST /api/upload/init
Content-Type: application/json
```

**Request Body:**

```json
{
  "filename": "large-image.jpg",
  "mime_type": "image/jpeg",
  "total_size": 10485760
}
```

**Response (201 Created):**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "in_progress",
  "received_bytes": 0,
  "total_size": 10485760,
  "progress": 0.0,
  "chunk_size": 5242880,
  "next_offset": 0,
  "expires_at": "2024-01-01T12:00:00Z"
}
```

---

#### Upload Chunk

Upload a chunk of data to an active session.

```
PATCH /api/upload/{session_id}/chunk
Content-Type: application/octet-stream
Content-Range: bytes {start}-{end}/{total}
```

**Request:**

```bash
curl -X PATCH "http://localhost:3000/api/upload/{session_id}/chunk" \
  -H "Content-Range: bytes 0-5242879/10485760" \
  --data-binary @chunk1.bin
```

**Response (200 OK):**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "in_progress",
  "received_bytes": 5242880,
  "total_size": 10485760,
  "progress": 50.0,
  "chunk_size": 5242880,
  "next_offset": 5242880,
  "expires_at": "2024-01-01T12:00:00Z"
}
```

---

#### Complete Upload

Finalize a chunked upload.

```
POST /api/upload/{session_id}/complete
```

**Response (200 OK):**

Same as simple upload response.

---

#### Get Upload Status

Check progress of a chunked upload (for resuming).

```
GET /api/upload/{session_id}/status
```

**Response (200 OK):**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "in_progress",
  "received_bytes": 5242880,
  "total_size": 10485760,
  "progress": 50.0,
  "chunk_size": 5242880,
  "next_offset": 5242880,
  "expires_at": "2024-01-01T12:00:00Z"
}
```

**Session Statuses:**

| Status | Description |
|--------|-------------|
| `in_progress` | Accepting chunks |
| `processing` | All chunks received, processing |
| `completed` | Upload finished successfully |
| `failed` | Upload failed (see error) |
| `expired` | Session timed out |
| `cancelled` | Cancelled by client |

---

### Media Serving

#### Get Optimized Media

Serve the WebP-optimized version.

```
GET /m/{media_id}
```

**Response Headers:**

```
Content-Type: image/webp
Cache-Control: public, max-age=31536000, immutable
ETag: "abc123..."
```

**Caching:**

- Response is cached for 1 year
- `If-None-Match` header supported → returns 304 Not Modified

---

#### Get Original Media

Serve the original uploaded file.

```
GET /m/{media_id}/original
```

**Response Headers:**

```
Content-Type: image/jpeg
Cache-Control: public, max-age=31536000, immutable
ETag: "abc123..."
Content-Disposition: inline; filename="original-name.jpg"
```

---

### Health Checks

#### Liveness Probe

Check if server is running.

```
GET /health/live
```

**Response:**

```json
{
  "status": "ok",
  "version": "0.1.0",
  "uptime": "running"
}
```

---

#### Readiness Probe

Check if server can accept requests.

```
GET /health/ready
```

**Response:**

```json
{
  "status": "ready",
  "database": "connected"
}
```

---

#### Stats

Get basic statistics.

```
GET /health/stats
```

**Response:**

```json
{
  "media_count": 1234,
  "storage": {
    "originals_size": 1073741824,
    "optimized_size": 536870912,
    "temp_size": 0,
    "total_size": 1610612736,
    "originals_count": 1234,
    "optimized_count": 1234
  }
}
```

---

## Admin API

> ⚠️ **Admin API is bound to 127.0.0.1 only and should never be exposed publicly.**

### Delete Media

Permanently remove a media file (for content moderation).

```
DELETE /admin/media/{media_id}
```

**Response (200 OK):**

```json
{
  "success": true,
  "message": "Media 550e8400-e29b-41d4-a716-446655440000 deleted successfully",
  "id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### Get Media Info

Get detailed metadata about a media file.

```
GET /admin/media/{media_id}
```

**Response:**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "original_filename": "photo.jpg",
  "original_mime_type": "image/jpeg",
  "optimized_mime_type": "image/webp",
  "media_type": "image",
  "original_size": 1048576,
  "optimized_size": 524288,
  "width": 1920,
  "height": 1080,
  "content_hash": "abc123...",
  "created_at": "2024-01-01T10:00:00Z",
  "last_accessed_at": "2024-01-01T11:00:00Z",
  "url": "http://localhost:3000/m/550e8400-e29b-41d4-a716-446655440000"
}
```

---

### Get Stats

Get detailed storage statistics.

```
GET /admin/stats
```

**Response:**

```json
{
  "media_count": 1234,
  "storage": {
    "originals_size": 1073741824,
    "optimized_size": 536870912,
    "temp_size": 10485760,
    "total_size": 1620590592,
    "originals_count": 1234,
    "optimized_count": 1234
  }
}
```

---

### Cleanup Sessions

Remove expired upload sessions and temp files.

```
POST /admin/cleanup
```

**Response:**

```json
{
  "sessions_cleaned": 5,
  "files_cleaned": 5,
  "orphaned_dirs_cleaned": 2
}
```

---

## Error Response Format

All errors return a consistent JSON format:

```json
{
  "error": "error_type",
  "message": "Human-readable error message",
  "status": 400
}
```

**Error Types:**

| Type | HTTP Status | Description |
|------|-------------|-------------|
| `validation_error` | 400 | Invalid input |
| `not_found` | 404 | Resource not found |
| `unsupported_media_type` | 415 | File type not allowed |
| `payload_too_large` | 413 | File too large |
| `rate_limit_exceeded` | 429 | Too many requests |
| `upload_session_error` | 400 | Session expired/invalid |
| `internal_error` | 500 | Server error |

---

## Rate Limiting

Default limits (configurable):

- **100 requests** per 60 seconds (general)
- **20 uploads** per 60 seconds

Rate limit headers:

```
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 95
X-RateLimit-Reset: 1704067200
```

