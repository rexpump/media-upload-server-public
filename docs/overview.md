# Architecture Overview

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Media Upload Server                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   ┌─────────────────────────────────────────────────────────────┐   │
│   │                      HTTP Layer (Axum)                       │   │
│   │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │   │
│   │  │ Public API  │  │ Serve API   │  │ Admin API (local)   │  │   │
│   │  │ :3000       │  │ :3000       │  │ :3001 (127.0.0.1)   │  │   │
│   │  └─────────────┘  └─────────────┘  └─────────────────────┘  │   │
│   └─────────────────────────────────────────────────────────────┘   │
│                                │                                     │
│   ┌─────────────────────────────────────────────────────────────┐   │
│   │                     Service Layer                            │   │
│   │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐   │   │
│   │  │   Storage    │  │    Image     │  │    Database      │   │   │
│   │  │   Service    │  │  Processor   │  │    Service       │   │   │
│   │  └──────────────┘  └──────────────┘  └──────────────────┘   │   │
│   └─────────────────────────────────────────────────────────────┘   │
│                                │                                     │
│   ┌─────────────────────────────────────────────────────────────┐   │
│   │                    Storage Layer                             │   │
│   │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐   │   │
│   │  │  Originals   │  │  Optimized   │  │    RocksDB       │   │   │
│   │  │   (files)    │  │   (WebP)     │  │   (metadata)     │   │   │
│   │  └──────────────┘  └──────────────┘  └──────────────────┘   │   │
│   └─────────────────────────────────────────────────────────────┘   │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Design Principles

### 1. Simplicity

- Single binary deployment
- RocksDB for metadata (embedded, crash-safe)
- File-based storage
- Minimal configuration required

### 2. Performance

- Async I/O with Tokio
- Streaming file uploads (no memory buffering)
- WebP conversion for smaller file sizes
- Efficient caching headers

### 3. Security

- Magic byte validation (don't trust Content-Type)
- UUID-based file names (no path traversal)
- Admin API on localhost only
- EXIF stripping (no metadata leaks)
- Content-Security-Policy headers

### 4. Reliability

- Chunked uploads with resume capability
- Content deduplication (hash-based)
- Graceful error handling
- Automatic cleanup of expired sessions

## Component Details

### Storage Service

Handles all file system operations:

```
data/
├── originals/           # Original uploaded files
│   ├── {uuid}.jpg
│   ├── {uuid}.png
│   └── ...
├── optimized/           # WebP-converted files
│   ├── {uuid}.webp
│   └── ...
├── temp/                # Chunked upload temp files
│   └── {session_uuid}/
│       └── upload       # Assembled chunks
└── rocksdb/             # RocksDB database
```

### Image Processor

Responsible for:

1. **Format Detection** - Uses magic bytes, not file extension
2. **Validation** - Only allows JPEG, PNG, GIF, WebP
3. **Resizing** - Limits max dimension to prevent DoS
4. **WebP Conversion** - Optimizes for web delivery
5. **EXIF Stripping** - Removes metadata for privacy

### Database Service

RocksDB key-value store with column families:

**media** - Stores metadata about uploaded files
- UUID primary key
- Original and optimized file info
- Dimensions, sizes, content hash
- Timestamps

**upload_sessions** - Tracks chunked uploads
- Session state and progress
- Expiration handling
- Error tracking

## Data Flow

### Simple Upload

```
Client                    Server
  │                         │
  │ POST /api/upload        │
  │ [multipart form]        │
  │────────────────────────>│
  │                         │ 1. Validate size
  │                         │ 2. Detect MIME type
  │                         │ 3. Calculate hash
  │                         │ 4. Check for duplicate
  │                         │ 5. Process image (WebP)
  │                         │ 6. Save files
  │                         │ 7. Insert DB record
  │                         │
  │<────────────────────────│
  │ 201 Created             │
  │ {id, url, ...}          │
```

### Chunked Upload

```
Client                    Server
  │                         │
  │ POST /api/upload/init   │
  │────────────────────────>│
  │                         │ Create session
  │<────────────────────────│
  │ {session_id, ...}       │
  │                         │
  │ PATCH /upload/{id}/chunk│
  │ [bytes 0-5MB]           │
  │────────────────────────>│
  │                         │ Append to temp file
  │<────────────────────────│
  │ {progress: 50%}         │
  │                         │
  │ ... more chunks ...     │
  │                         │
  │ POST /upload/{id}/complete
  │────────────────────────>│
  │                         │ Process & store
  │<────────────────────────│
  │ {id, url, ...}          │
```

### Serving Media

```
Client                    Server
  │                         │
  │ GET /m/{id}             │
  │────────────────────────>│
  │                         │ 1. Get metadata
  │                         │ 2. Check ETag
  │                         │ 3. Stream file
  │<────────────────────────│
  │ 200 OK                  │
  │ [WebP image]            │
  │ Cache-Control: immutable│
```

## Security Considerations

### Input Validation

1. **File Type Validation**
   - Uses `infer` crate for magic byte detection
   - Never trusts Content-Type header
   - Whitelist of allowed types

2. **Size Limits**
   - Configurable per-request limit
   - Separate limits for simple vs chunked

3. **Filename Sanitization**
   - All files stored with UUID names
   - Original filename only in database

### Access Control

1. **Public API** (port 3000)
   - Anyone can upload (rate limited)
   - Anyone can view uploaded media

2. **Admin API** (port 3001)
   - Bound to 127.0.0.1 only
   - Can delete any media
   - Access to stats and cleanup

### Data Protection

1. **EXIF Stripping**
   - Removes GPS coordinates
   - Removes camera info
   - Removes other metadata

2. **Content Deduplication**
   - Based on content hash
   - Prevents duplicate storage
   - Returns existing URL for duplicates

