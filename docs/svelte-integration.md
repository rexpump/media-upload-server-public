# Svelte/SvelteKit Integration Guide

Интеграция медиа-сервера с фронтендом на Svelte/SvelteKit.

## Установка

Для загрузки файлов не нужны дополнительные зависимости — используем нативный `fetch`.

## Базовый компонент загрузки

### `ImageUpload.svelte`

```svelte
<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  // Конфигурация
  export let apiUrl = 'http://localhost:3000';
  export let apiKey: string | null = null;
  export let maxSize = 50 * 1024 * 1024; // 50MB
  export let accept = 'image/jpeg,image/png,image/gif,image/webp';

  const dispatch = createEventDispatcher<{
    success: { id: string; url: string; originalUrl?: string };
    error: { message: string };
    progress: { percent: number };
  }>();

  let uploading = false;
  let progress = 0;
  let dragover = false;
  let fileInput: HTMLInputElement;

  // Валидация файла
  function validateFile(file: File): string | null {
    if (file.size > maxSize) {
      return `Файл слишком большой. Максимум: ${Math.round(maxSize / 1024 / 1024)}MB`;
    }
    
    const allowedTypes = accept.split(',').map(t => t.trim());
    if (!allowedTypes.includes(file.type)) {
      return `Неподдерживаемый формат. Разрешены: ${accept}`;
    }
    
    return null;
  }

  // Загрузка файла
  async function uploadFile(file: File) {
    const error = validateFile(file);
    if (error) {
      dispatch('error', { message: error });
      return;
    }

    uploading = true;
    progress = 0;

    try {
      const formData = new FormData();
      formData.append('file', file);

      const headers: Record<string, string> = {};
      if (apiKey) {
        headers['Authorization'] = `Bearer ${apiKey}`;
      }

      const response = await fetch(`${apiUrl}/api/upload`, {
        method: 'POST',
        headers,
        body: formData,
      });

      if (!response.ok) {
        const errorData = await response.json();
        throw new Error(errorData.message || 'Ошибка загрузки');
      }

      const result = await response.json();
      
      dispatch('success', {
        id: result.id,
        url: result.url,
        originalUrl: result.original_url,
      });

      progress = 100;
    } catch (err) {
      dispatch('error', { 
        message: err instanceof Error ? err.message : 'Неизвестная ошибка' 
      });
    } finally {
      uploading = false;
    }
  }

  // Обработка выбора файла
  function handleFileSelect(event: Event) {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (file) {
      uploadFile(file);
    }
  }

  // Обработка drag & drop
  function handleDrop(event: DragEvent) {
    event.preventDefault();
    dragover = false;
    
    const file = event.dataTransfer?.files[0];
    if (file) {
      uploadFile(file);
    }
  }

  function handleDragOver(event: DragEvent) {
    event.preventDefault();
    dragover = true;
  }

  function handleDragLeave() {
    dragover = false;
  }
</script>

<div 
  class="upload-zone"
  class:dragover
  class:uploading
  on:drop={handleDrop}
  on:dragover={handleDragOver}
  on:dragleave={handleDragLeave}
  role="button"
  tabindex="0"
  on:click={() => fileInput.click()}
  on:keypress={(e) => e.key === 'Enter' && fileInput.click()}
>
  <input
    bind:this={fileInput}
    type="file"
    {accept}
    on:change={handleFileSelect}
    hidden
  />
  
  {#if uploading}
    <div class="progress">
      <div class="progress-bar" style="width: {progress}%"></div>
    </div>
    <p>Загрузка...</p>
  {:else}
    <p>📷 Перетащите изображение или кликните для выбора</p>
    <p class="hint">Макс. размер: {Math.round(maxSize / 1024 / 1024)}MB</p>
  {/if}
</div>

<style>
  .upload-zone {
    border: 2px dashed #ccc;
    border-radius: 12px;
    padding: 2rem;
    text-align: center;
    cursor: pointer;
    transition: all 0.2s ease;
    background: #fafafa;
  }

  .upload-zone:hover,
  .upload-zone.dragover {
    border-color: #007bff;
    background: #f0f7ff;
  }

  .upload-zone.uploading {
    pointer-events: none;
    opacity: 0.7;
  }

  .progress {
    height: 8px;
    background: #e0e0e0;
    border-radius: 4px;
    overflow: hidden;
    margin-bottom: 1rem;
  }

  .progress-bar {
    height: 100%;
    background: #007bff;
    transition: width 0.3s ease;
  }

  .hint {
    font-size: 0.85rem;
    color: #666;
  }
</style>
```

## Использование компонента

```svelte
<script lang="ts">
  import ImageUpload from './ImageUpload.svelte';

  let uploadedImage: { url: string; id: string } | null = null;

  function handleSuccess(event: CustomEvent) {
    uploadedImage = event.detail;
    console.log('Uploaded:', uploadedImage);
  }

  function handleError(event: CustomEvent) {
    alert(event.detail.message);
  }
</script>

<ImageUpload
  apiUrl="https://media.yoursite.com"
  apiKey="your-api-key-here"
  on:success={handleSuccess}
  on:error={handleError}
/>

{#if uploadedImage}
  <div class="preview">
    <img src={uploadedImage.url} alt="Uploaded" />
  </div>
{/if}
```

## Chunked Upload для больших файлов

### `ChunkedUpload.svelte`

```svelte
<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let apiUrl = 'http://localhost:3000';
  export let apiKey: string | null = null;
  export let chunkSize = 5 * 1024 * 1024; // 5MB chunks

  const dispatch = createEventDispatcher<{
    success: { id: string; url: string };
    error: { message: string };
    progress: { percent: number; uploaded: number; total: number };
  }>();

  let uploading = false;
  let progress = 0;
  let uploadedBytes = 0;
  let totalBytes = 0;

  async function uploadChunked(file: File) {
    uploading = true;
    progress = 0;
    uploadedBytes = 0;
    totalBytes = file.size;

    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    };
    if (apiKey) {
      headers['Authorization'] = `Bearer ${apiKey}`;
    }

    try {
      // 1. Инициализация сессии
      const initResponse = await fetch(`${apiUrl}/api/upload/init`, {
        method: 'POST',
        headers,
        body: JSON.stringify({
          filename: file.name,
          mime_type: file.type,
          total_size: file.size,
        }),
      });

      if (!initResponse.ok) {
        const error = await initResponse.json();
        throw new Error(error.message || 'Failed to init upload');
      }

      const session = await initResponse.json();
      const sessionId = session.id;

      // 2. Загрузка chunks
      let offset = 0;
      while (offset < file.size) {
        const chunk = file.slice(offset, offset + chunkSize);
        const end = Math.min(offset + chunkSize - 1, file.size - 1);

        const chunkResponse = await fetch(
          `${apiUrl}/api/upload/${sessionId}/chunk`,
          {
            method: 'PATCH',
            headers: {
              'Content-Range': `bytes ${offset}-${end}/${file.size}`,
              ...(apiKey ? { Authorization: `Bearer ${apiKey}` } : {}),
            },
            body: chunk,
          }
        );

        if (!chunkResponse.ok) {
          const error = await chunkResponse.json();
          throw new Error(error.message || 'Chunk upload failed');
        }

        offset += chunkSize;
        uploadedBytes = Math.min(offset, file.size);
        progress = Math.round((uploadedBytes / totalBytes) * 100);
        
        dispatch('progress', { 
          percent: progress, 
          uploaded: uploadedBytes, 
          total: totalBytes 
        });
      }

      // 3. Завершение загрузки
      const completeResponse = await fetch(
        `${apiUrl}/api/upload/${sessionId}/complete`,
        {
          method: 'POST',
          headers: apiKey ? { Authorization: `Bearer ${apiKey}` } : {},
        }
      );

      if (!completeResponse.ok) {
        const error = await completeResponse.json();
        throw new Error(error.message || 'Failed to complete upload');
      }

      const result = await completeResponse.json();
      dispatch('success', { id: result.id, url: result.url });

    } catch (err) {
      dispatch('error', { 
        message: err instanceof Error ? err.message : 'Upload failed' 
      });
    } finally {
      uploading = false;
    }
  }

  function handleFileSelect(event: Event) {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (file) {
      uploadChunked(file);
    }
  }
</script>

<div class="chunked-upload">
  <input type="file" on:change={handleFileSelect} disabled={uploading} />
  
  {#if uploading}
    <div class="progress-container">
      <div class="progress-bar" style="width: {progress}%"></div>
      <span class="progress-text">
        {progress}% ({Math.round(uploadedBytes / 1024 / 1024)}MB / {Math.round(totalBytes / 1024 / 1024)}MB)
      </span>
    </div>
  {/if}
</div>

<style>
  .progress-container {
    margin-top: 1rem;
    background: #e0e0e0;
    border-radius: 8px;
    overflow: hidden;
    position: relative;
    height: 24px;
  }

  .progress-bar {
    height: 100%;
    background: linear-gradient(90deg, #007bff, #00d4ff);
    transition: width 0.3s ease;
  }

  .progress-text {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    font-size: 0.85rem;
    font-weight: 500;
  }
</style>
```

## API Service (TypeScript)

### `lib/mediaService.ts`

```typescript
interface UploadResult {
  id: string;
  url: string;
  original_url?: string;
  media_type: 'image' | 'video';
  mime_type: string;
  size: number;
  width: number;
  height: number;
}

interface UploadOptions {
  apiKey?: string;
  onProgress?: (percent: number) => void;
}

export class MediaService {
  constructor(private baseUrl: string) {}

  async upload(file: File, options: UploadOptions = {}): Promise<UploadResult> {
    const formData = new FormData();
    formData.append('file', file);

    const headers: Record<string, string> = {};
    if (options.apiKey) {
      headers['Authorization'] = `Bearer ${options.apiKey}`;
    }

    const response = await fetch(`${this.baseUrl}/api/upload`, {
      method: 'POST',
      headers,
      body: formData,
    });

    if (!response.ok) {
      const error = await response.json();
      throw new Error(error.message || 'Upload failed');
    }

    return response.json();
  }

  async uploadChunked(
    file: File,
    options: UploadOptions & { chunkSize?: number } = {}
  ): Promise<UploadResult> {
    const chunkSize = options.chunkSize || 5 * 1024 * 1024;
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    };
    if (options.apiKey) {
      headers['Authorization'] = `Bearer ${options.apiKey}`;
    }

    // Init session
    const initResponse = await fetch(`${this.baseUrl}/api/upload/init`, {
      method: 'POST',
      headers,
      body: JSON.stringify({
        filename: file.name,
        mime_type: file.type,
        total_size: file.size,
      }),
    });

    if (!initResponse.ok) {
      throw new Error('Failed to init upload');
    }

    const { id: sessionId } = await initResponse.json();

    // Upload chunks
    let offset = 0;
    while (offset < file.size) {
      const chunk = file.slice(offset, offset + chunkSize);
      const end = Math.min(offset + chunkSize - 1, file.size - 1);

      const chunkResponse = await fetch(
        `${this.baseUrl}/api/upload/${sessionId}/chunk`,
        {
          method: 'PATCH',
          headers: {
            'Content-Range': `bytes ${offset}-${end}/${file.size}`,
            ...(options.apiKey ? { Authorization: `Bearer ${options.apiKey}` } : {}),
          },
          body: chunk,
        }
      );

      if (!chunkResponse.ok) {
        throw new Error('Chunk upload failed');
      }

      offset += chunkSize;
      options.onProgress?.(Math.round((offset / file.size) * 100));
    }

    // Complete
    const completeResponse = await fetch(
      `${this.baseUrl}/api/upload/${sessionId}/complete`,
      {
        method: 'POST',
        headers: options.apiKey ? { Authorization: `Bearer ${options.apiKey}` } : {},
      }
    );

    if (!completeResponse.ok) {
      throw new Error('Failed to complete upload');
    }

    return completeResponse.json();
  }

  getMediaUrl(id: string): string {
    return `${this.baseUrl}/m/${id}`;
  }

  getOriginalUrl(id: string): string {
    return `${this.baseUrl}/m/${id}/original`;
  }
}

// Singleton instance
export const mediaService = new MediaService(
  import.meta.env.VITE_MEDIA_API_URL || 'http://localhost:3000'
);
```

## SvelteKit Server-Side (API Route)

### `routes/api/upload/+server.ts`

```typescript
import { json, error } from '@sveltejs/kit';
import type { RequestHandler } from './$types';

const MEDIA_API_URL = process.env.MEDIA_API_URL || 'http://localhost:3000';
const MEDIA_API_KEY = process.env.MEDIA_API_KEY;

export const POST: RequestHandler = async ({ request }) => {
  try {
    const formData = await request.formData();
    
    const headers: Record<string, string> = {};
    if (MEDIA_API_KEY) {
      headers['Authorization'] = `Bearer ${MEDIA_API_KEY}`;
    }

    const response = await fetch(`${MEDIA_API_URL}/api/upload`, {
      method: 'POST',
      headers,
      body: formData,
    });

    if (!response.ok) {
      const errorData = await response.json();
      throw error(response.status, errorData.message);
    }

    const result = await response.json();
    return json(result);
  } catch (err) {
    console.error('Upload error:', err);
    throw error(500, 'Upload failed');
  }
};
```

## Environment Variables

### `.env`

```bash
# Media server URL
VITE_MEDIA_API_URL=http://localhost:3000

# API key (для серверной части)
MEDIA_API_KEY=your-secure-api-key
```

## Советы по безопасности

### 1. API ключи на сервере

Храните API ключи только на сервере (SvelteKit server routes), не в клиентском коде:

```typescript
// ❌ НЕ делайте так
const apiKey = 'secret-key'; // Виден в браузере!

// ✅ Делайте так - через серверный route
// routes/api/upload/+server.ts
const apiKey = process.env.MEDIA_API_KEY; // Только на сервере
```

### 2. Валидация на клиенте

Валидируйте файлы перед отправкой:

```typescript
function validateFile(file: File): boolean {
  const maxSize = 50 * 1024 * 1024; // 50MB
  const allowedTypes = ['image/jpeg', 'image/png', 'image/gif', 'image/webp'];
  
  if (file.size > maxSize) {
    alert('Файл слишком большой');
    return false;
  }
  
  if (!allowedTypes.includes(file.type)) {
    alert('Неподдерживаемый формат');
    return false;
  }
  
  return true;
}
```

### 3. CORS

Настройте CORS на медиа-сервере для вашего домена в продакшене.

## Примеры использования

### Простая загрузка

```svelte
<script>
  import { mediaService } from '$lib/mediaService';
  
  let imageUrl = '';
  
  async function handleUpload(event) {
    const file = event.target.files[0];
    try {
      const result = await mediaService.upload(file);
      imageUrl = result.url;
    } catch (err) {
      console.error(err);
    }
  }
</script>

<input type="file" on:change={handleUpload} accept="image/*" />
{#if imageUrl}
  <img src={imageUrl} alt="Uploaded" />
{/if}
```

### С прогрессом

```svelte
<script>
  import { mediaService } from '$lib/mediaService';
  
  let progress = 0;
  let imageUrl = '';
  
  async function handleUpload(event) {
    const file = event.target.files[0];
    progress = 0;
    
    try {
      const result = await mediaService.uploadChunked(file, {
        onProgress: (p) => progress = p,
      });
      imageUrl = result.url;
    } catch (err) {
      console.error(err);
    }
  }
</script>

<input type="file" on:change={handleUpload} />
{#if progress > 0 && progress < 100}
  <progress value={progress} max="100">{progress}%</progress>
{/if}
{#if imageUrl}
  <img src={imageUrl} alt="Uploaded" />
{/if}
```

