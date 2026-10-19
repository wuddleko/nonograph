---
title: API Documentation
date: 2026-10-13
author: nostorgraph
generator: nostorgraph v0.4.6
---

A note is sealed, then the ciphertext is sent to Nostr relays. The host does not keep a copy of the writing. The link you get carries the key.

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
| `csrf_token` | When CSRF is on | Must match the `nostorgraph_csrf` cookie. |

The relays are the `nostorgraph_relays` cookie, edited with `POST /nojs/relays`. A public relay is `wss://` on port 443, with no username or password, and not a local or private address. Private URLs in the cookie are skipped. At most 6 public relays are used.

### Success
**Status:** `303` redirect to `/s/{locator}/{event-id}#key`

The key is the URL fragment. The host does not store it, the writing, or where the note was sent. `?r=` is added for every accepted relay, at most six. `/s/{locator}#key` opens a short link after that link has been saved.

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
  "seal_claim": "",
  "csrf_token": ""
}
```

`event` must already be a sealed note (kind 30323) with a valid signature and stamp. This endpoint does not accept plaintext. `relays` are extra public relays. The instance relays come from `Config.toml` and take the slots first. At most 6 public relays are used, so a later extra relay is dropped. Private relay URLs are dropped. `seal_claim` is an HMAC of the key and the locator. Before the note reaches a relay, the host stores a hash of that claim. It does not store the event id, the relays, the writing, or the key. If no public relay accepts the note, the hash is dropped. A short or title link can be saved later only with the same claim. When CSRF is on, `csrf_token` must match the `nostorgraph_csrf` cookie.

### Success
**Status:** `200`
```json
{ "locator": "0123456789abcdef0123", "accepted": ["wss://relay.example"], "hints": ["wss://relay.example"] }
```

`locator` is empty when no public relay accepted the note. That response is still 200. `hints` lists every accepted relay, at most six. The long share link is `/s/{locator}/{event-id}` with each hint as `?r=`.

`POST /seal-row` saves a short link. `POST /title-link` saves a title link and returns `{ "slug": "..." }`. Both send the locator, event id, relays, and `claim`, the same HMAC. The host checks the claim against the hash stored at publish, then stores the event id and the relays. It does not store the writing or the key. A claim that does not match that hash, or a link already saved for that locator, is `400`. A busy publish slot is `503`. `/seal-row` returns `204`.

### Failure
| Status | When |
|--------|------|
| `400` | CSRF failed, the event is not a sealed note, or no public relay remains |
| `422` | The body is not the JSON object above |
| `503` | The publish slot is busy, or the publish task failed |

## Reading
`GET /s/{locator}/{event-id}` opens a long link. Each `?r=` parameter names a relay to ask. The host fetches that event and does not decrypt it or keep it. `GET /s/{locator}` opens a short link from the event id and relays saved for that locator. Extra query parameters do not choose a different event. A locator with no saved short link is **404**. The browser opens the note. The key stays in the fragment.

`GET /nojs/s/{locator}` and `GET /nojs/s/{locator}/{event-id}` show a page that says to run locally and redirects to the source code. A bad locator or event id is **404**.

The site pages `/about`, `/legal`, `/api`, and `/markup` are the only pages stored here. Their no-JS addresses are `/nojs/about` and the same for the other three.
