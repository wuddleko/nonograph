---
title: API Documentation
date: 2026-10-10
author: nonograph
generator: nonograph v0.2.8
---

Publishing does not write a file on this host. A note is sealed, then the ciphertext is sent to Nostr relays. The host does not keep a copy of the writing. The link you get carries the key.

There are two ways in.

## No JavaScript

**Endpoint:** `POST /nojs/create`
**Content-Type:** `application/x-www-form-urlencoded`

The host seals the note and publishes it. The form on `/nojs` is this request.

### Parameters
| Field | Required | Description |
|-------|----------|-------------|
| `title` | Yes | Title. Trimmed. Default limit 128 characters. |
| `content` | Yes | Markup. Not trimmed. Default limit 128,000 characters. |
| `alias` | No | Optional name. Trimmed. Default limit 32 characters. |
| `csrf_token` | When CSRF is on | Token from the form. |
| `relay` | No | One extra public `wss://` relay. A private or local URL is rejected. |
| `relay_box` | No | Same check as `relay`. The box wins when both are set. |

### Success
**Status:** `303` redirect to `/s/{locator}/{event-id}#key`

The key is the URL fragment. The host does not store it. A relay you added is included as `?r=`.

### Failure
The form is rendered again with an error. Common codes: `title_required`, `content_required`, `title_too_long`, `content_too_long`, `alias_too_long`, `csrf_token_invalid`, `relay_invalid`, `nostr_publish_failed`.

## JavaScript

The browser seals the note, then posts the sealed event.

**Endpoint:** `POST /nostr/publish`
**Content-Type:** `application/json`

```json
{
  "event": {},
  "relays": ["wss://relay.example"],
  "csrf_token": ""
}
```

`event` must already be a sealed note (kind 30323) with a valid signature and stamp. This endpoint does not accept plaintext. `relays` are extra public relays; the instance relays come from `Config.toml`. Private relay URLs are dropped.

### Success
**Status:** `200`
```json
{ "locator": "0123456789abcdef0123", "accepted": ["wss://relay.example"] }
```

`locator` is empty when no public relay accepted the note.

### Failure
| Status | When |
|--------|------|
| `400` | CSRF failed, the event is not a sealed note, or no public relay remains |
| `503` | The publish slot is busy, or the publish task failed |

## Reading
A sealed note opens at `/s/{locator}/{event-id}` with the key in the fragment. `/nojs/s/...` is the same page without scripts. The host fetches the ciphertext from relays and does not keep it.

The site pages `/about`, `/legal`, `/api`, and `/markup` are the only pages stored here. Their no-JS addresses are `/nojs/about` and the same for the other three.
