---
title: API Documentation
date: 2026-10-13
author: nonograph
generator: nonograph v0.4.6
---

Publishing does not write a file on this host. A note is sealed, then the ciphertext is sent to Nostr relays. The host does not keep a copy of the writing. The link you get carries the key.

There are two ways in.

## Limits

Title, alias, and content lengths come from `[limits]` in `Config.toml`. Each check counts characters. When that file is missing, the limits are 128, 32, and 128,000. This repository's file sets the content limit to 256,000.

Title and alias are trimmed before the check. A blank alias is omitted. Content is not trimmed: a value that is empty after trimming is `content_required`, and any other value is sealed with its spaces kept.

## No JavaScript

**Endpoint:** `POST /nojs/create`
**Content-Type:** `application/x-www-form-urlencoded`

The host seals the note and publishes it. The form on `/nojs` is this request.

### Parameters
| Field | Required | Description |
|-------|----------|-------------|
| `title` | Yes | Title. Trimmed. |
| `content` | Yes | Markup. Not trimmed. |
| `alias` | No | Optional name. Trimmed. |
| `csrf_token` | When CSRF is on | Must match the `nonograph_csrf` cookie. |

The relays are the `nonograph_relays` cookie, edited with `POST /nojs/relays`. A public relay is `wss://` on port 443, with no username or password, and not a local or private address. Private URLs in the cookie are skipped. At most 6 public relays are used.

### Success
**Status:** `303` redirect to `/s/{locator}/{event-id}#key`

The key is the URL fragment. The host does not store it. `?r=` is added for each accepted relay that is not already listed in `Config.toml`.

### Failure
The form is rendered again with an error. Common codes: `title_required`, `content_required`, `title_too_long`, `content_too_long`, `alias_too_long`, `csrf_token_invalid`, `nostr_publish_failed`. An empty relay cookie is `nostr_publish_failed`.

A busy publish slot is `nostr_publish_failed` on this path.

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

`event` must already be a sealed note (kind 30323) with a valid signature and stamp. This endpoint does not accept plaintext. `relays` are extra public relays. The instance relays come from `Config.toml` and take the slots first. At most 6 public relays are used, so a later extra relay is dropped. Private relay URLs are dropped. When CSRF is on, `csrf_token` must match the `nonograph_csrf` cookie.

### Success
**Status:** `200`
```json
{ "locator": "0123456789abcdef0123", "accepted": ["wss://relay.example"] }
```

`locator` is empty when no public relay accepted the note. That response is still 200.

### Failure
| Status | When |
|--------|------|
| `400` | CSRF failed, the event is not a sealed note, or no public relay remains |
| `422` | The body is not the JSON object above |
| `503` | The publish slot is busy, or the publish task failed |

## Reading
`GET /s/{locator}/{event-id}` opens a sealed note with the key in the fragment. The host fetches the ciphertext from relays and does not decrypt it or keep it. The browser opens the note.

`GET /nojs/s/{locator}/{event-id}` does not open the note. The page says to run locally and redirects to the source code. A bad locator or event id is **404**.

The site pages `/about`, `/legal`, `/api`, and `/markup` are the only pages stored here. Their no-JS addresses are `/nojs/about` and the same for the other three.
