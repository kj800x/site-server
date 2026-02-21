# Remote Site API Protocol v1

A remote site server exposes three HTTP endpoints under a configurable base URL prefix. The consuming client (site-server) fetches all item metadata into memory on startup, then proxies individual asset requests at runtime.

## Endpoints

All endpoints are under `{base_url}` (e.g. `https://remote-server.example.com/api`).

### 1. `GET {base_url}/v1/version`

Returns the protocol version and a change-detection timestamp.

**Response:** `200 OK`, `Content-Type: application/json`

```json
{
  "protocol": 1,
  "lastUpdatedAt": 1708531200000
}
```

| Field | Type | Description |
|-------|------|-------------|
| `protocol` | integer | Must be `1`. The client rejects other values. |
| `lastUpdatedAt` | integer | Unix timestamp (milliseconds). The client polls this endpoint periodically (every 60s) and re-fetches items when this value increases. |

### 2. `GET {base_url}/v1/items`

Returns the full item catalog. The client deserializes this into memory on startup and on each reload.

**Response:** `200 OK`, `Content-Type: application/json`

The body is a **JSON array** of `CrawlItem` objects (not a map/object):

```json
[
  { "title": "...", "key": "...", ... },
  { "title": "...", "key": "...", ... }
]
```

#### CrawlItem schema

All field names use **camelCase**.

```json
{
  "title": "Example Post",
  "key": "unique-item-key",
  "url": "https://original-source.com/post/123",
  "description": { "format": "plaintext", "value": "Description text here" },
  "meta": {},
  "sourcePublished": 1708531200000,
  "firstSeen": 1708531200000,
  "lastSeen": 1708531200000,
  "seenInLastRefresh": true,
  "tags": ["tag1", {"group": "category", "value": "tag2"}],
  "files": [
    {
      "type": "ImageFile",
      "key": "file1.jpg",
      "filename": "file1.jpg",
      "downloaded": true,
      "url": "https://original-source.com/file1.jpg"
    }
  ],
  "previews": [
    {
      "type": "ImageFile",
      "key": "preview1.jpg",
      "filename": "preview1.jpg",
      "downloaded": true,
      "url": "https://original-source.com/preview1.jpg"
    }
  ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Display title |
| `key` | string | **Unique identifier** for this item within the site. Used as map key after deserialization. |
| `url` | string | Original source URL |
| `description` | FormattedText | See FormattedText below |
| `meta` | any JSON value | Arbitrary metadata (pass-through, can be `{}`) |
| `sourcePublished` | integer (i64) | Milliseconds since Unix epoch. When the content was originally published. Can be `0` or `null` (defaults to `0`). |
| `firstSeen` | integer (u64) | Milliseconds since epoch. When this item was first crawled. |
| `lastSeen` | integer (u64) | Milliseconds since epoch. When this item was last seen in a crawl. |
| `seenInLastRefresh` | boolean | Whether this item was present in the most recent crawl |
| `tags` | array of Tag | See Tag below |
| `files` | array of FileCrawlType | The item's media files. **Must be an array**, not a map. Each element's `key` field is used as the map key after deserialization. |
| `previews` | array of FileCrawlType | Optional (defaults to `[]`). Thumbnail/preview files for listing pages. Same format as `files`. |

#### FormattedText

Internally tagged by `format` field (camelCase values):

```json
{"format": "plaintext", "value": "Hello world"}
{"format": "markdown", "value": "# Hello world"}
{"format": "html", "value": "<p>Hello world</p>"}
```

| Variant | Fields | Description |
|---------|--------|-------------|
| `plaintext` | `value: string` | Plain text |
| `markdown` | `value: string` | Markdown text |
| `html` | `value: string` | Raw HTML |

#### Tag

Untagged union — serde attempts each variant in order:

- **Detailed:** `{"group": "category", "value": "landscape"}` — an object with `group` and `value` string fields
- **Simple:** `"landscape"` — a bare string

Example array: `["simple-tag", {"group": "artist", "value": "someone"}]`

#### FileCrawlType

Internally tagged by `type` field. All field names use camelCase.

**ImageFile:**
```json
{
  "type": "ImageFile",
  "key": "photo1.jpg",
  "filename": "photo1.jpg",
  "downloaded": true,
  "url": "https://source.com/photo1.jpg"
}
```

**VideoFile:**
```json
{
  "type": "VideoFile",
  "key": "video1.mp4",
  "filename": "video1.mp4",
  "downloaded": true,
  "url": "https://source.com/video1.mp4"
}
```

**IntermediateFile** (container with nested files):
```json
{
  "type": "IntermediateFile",
  "key": "gallery1",
  "filename": "gallery1",
  "downloaded": true,
  "postprocessingErrors": false,
  "url": "https://source.com/gallery1",
  "nested": [
    { "type": "ImageFile", "key": "img1.jpg", "filename": "img1.jpg", "downloaded": true, "url": "..." },
    { "type": "ImageFile", "key": "img2.jpg", "filename": "img2.jpg", "downloaded": true, "url": "..." }
  ]
}
```

**InlineTextFile:**
```json
{
  "type": "InlineTextFile",
  "key": "text1",
  "content": "The actual text content inline"
}
```

| Variant | Fields | Notes |
|---------|--------|-------|
| `ImageFile` | `key`, `filename`, `downloaded`, `url` | |
| `VideoFile` | `key`, `filename`, `downloaded`, `url` | |
| `IntermediateFile` | `key`, `filename`, `downloaded`, `postprocessingErrors` (default `false`), `url`, `nested` (array of FileCrawlType) | `nested` is an array, not a map |
| `InlineTextFile` | `key`, `content` | Text content is inline, no external file |

**Important:** For all file types, `key` is the unique identifier within its parent container. The `filename` is the path segment used for asset requests. The `downloaded` flag must be `true` for the file to be displayed.

### 3. `GET {base_url}/v1/assets/{path}`

Serves asset files (images, videos, thumbnails). The `{path}` is a wildcard — it can contain slashes.

The client proxies requests from `/{site_slug}/assets/{path}` to `{base_url}/v1/assets/{path}`, forwarding the response status code and `Content-Type` header.

**Response:** The raw file bytes with an appropriate `Content-Type` header.

Examples:
- `GET {base_url}/v1/assets/photo1.jpg` → serves the image
- `GET {base_url}/v1/assets/thumbs/photo1.jpg` → serves a thumbnail
- `GET {base_url}/v1/assets/video1.mp4` → serves a video

The asset paths correspond to the `filename` fields in the items' `files` and `previews` arrays.

## Client Behavior

1. **Startup:** Fetches `/v1/version` (validates `protocol == 1`), then `/v1/items`. All items are loaded into memory.
2. **Polling:** Every 60 seconds, fetches `/v1/version` and compares `lastUpdatedAt` with the previously seen value. If it increased, re-fetches `/v1/items`.
3. **Asset proxy:** At request time, `GET /{slug}/assets/{path}` is proxied to `GET {base_url}/v1/assets/{path}`.
4. **Error handling:** If version/items fetch fails on startup, the server exits with an error. If polling fails, the error is logged and stale data continues to be served.

## Configuration

A remote site is configured by placing a `config.json` in a directory passed to the server:

```json
{
  "site": "https://example.com",
  "slug": "myremote",
  "label": "My Remote Site",
  "remote_url": "https://remote-server.example.com/api"
}
```

The presence of `remote_url` distinguishes remote from local sites. The directory does not need `crawled.json` or any asset files.

Optional config fields (same as local sites):
- `forced_author` (string | null) — override author display
- `hide_titles` (boolean, default false) — hide item titles in rendering
- `reprocessors` (array, default []) — post-processing transforms applied after loading
