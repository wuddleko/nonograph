<div align="center">
  <img align="center" width="96" height="96" alt="android-chrome-512x512" src="https://github.com/user-attachments/assets/9a06a3fe-46ee-422c-93ad-ce3e504603c0" />
</div>

<h1 align="center">Nonograph</h1>

<p align="center"><b>Anonymous publishing for the privacy-conscious web.</b></p>
<div align="center">
  <a href="https://unlicense.org">
    <img alt="GitHub License" src="https://img.shields.io/github/license/wuddleko/nonograph">
  </a>
  <a href="https://github.com/wuddleko/nonograph/releases/latest">
    <img alt="GitHub Release" src="https://img.shields.io/github/v/release/wuddleko/nonograph">
  </a>
  <a href="https://github.com/wuddleko/nonograph/commits/main/">
    <img alt="GitHub commit activity" src="https://img.shields.io/github/commit-activity/m/wuddleko/nonograph">
  </a>
  <a href="http://ortmy3ey5usdzf4ivht6axtb72owjniaeqrexknosyons544aooltzyd.onion/">
    <img src="https://img.shields.io/badge/Tor-Hidden%20Service-7d4698?style=flat&logo=torproject&logoColor=white" alt="Tor Hidden Service">
  </a>

English | [简体中文](README.zh.md)

</div>

Nonograph is a simple anonymous publishing platform for anyone who wants their words to exist online without being tied to an identity. You write, you get a shareable link, and that's it. No account creation, no profile, no analytics trail following readers around.

https://github.com/user-attachments/assets/d662c9a2-f0ed-4266-bf55-e2c1f024269e

## Nostr (WE NEED INSTANCES)

**On Nostr** signs your note in the browser, then this instance publishes it to the relays in `Config.toml` over Tor. What you share is a Nostr link—not a page stored on the server. Your browser never talks to relays; they see a Tor exit, not you. The instance still sees the normal HTTP request.

We could use more hosts running this setup. Pick relays that actually accept these posts (kind 30023); dead or read-only relays make the button look broken even when everything else is fine. If you're already running Nonograph, wire up `[nostr].relays`, deploy, and open a PR to add yourself to the table (clearnet and/or onion).

| Uptime | Location | Clearnet | Onion |
|--------|----------|----------|-------|
| _none listed yet_ | | | |

The original model—write on someone's box, get a link on their domain—is still going strong. Those instances are in the next section.

## Known instances

| Uptime                                                                               | Location      | Clearnet                         | Onion                                                                  |   |
|--------------------------------------------------------------------------------------|---------------|----------------------------------|------------------------------------------------------------------------|---|
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fnonogra.ph)              | 🏴‍☠️ Unknown    | https://nonogra.ph               | http://ortmy3ey5usdzf4ivht6axtb72owjniaeqrexknosyons544aooltzyd.onion/ |   |
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fwrite.eversiege.network) | 🏴‍☠️ Unknown    | https://write.eversiege.network/ | http://fmoigm7j3z6vh4hgssdfhlt6knkp443thgxpe5wmbaevvb5km2d3suyd.onion/ |   |
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fnonograph.com)           | 🇭🇰 Hong Kong  | https://nonograph.com            | http://gt65bmujun7alps7b7oar5x2u5lprxjdvpwrxcrnwkgheocjwllchhqd.onion/ |   |
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fproxy.write.daun.world)  | 🇫🇮 Finland    | https://proxy.write.daun.world/  | see above                                                              |   |
| Onion                                                                                | 🇷🇺 Russia     |                                  | http://q2w7sdjlmfc5vjif6y3372665wzyqskqvjji7bmhfn72orxcbljvonid.onion/ |   |
| Onion                                                                                | 🇭🇺 Hungary    |                                  | http://t7fgh7qvjysh3wer747m6dkjvkjsqvajyv5bh2grzjgpd2derxsxbdad.onion/ |   |
| Onion                                                                                | 🏴‍☠️ Unknown    |                                  | http://uawaa47jvsfr3ij63ns25xp6qvhqswsx3fgij2evbrcnt3ygxq3dbwyd.onion/ |   |
| Onion                                                                                | 🇰🇿 Kazakhstan |                                  | http://5mq3db45agipsceghnpx3iumlctya3absmp4sgnitqcmrmhaqhbbjcid.onion/ |   |

These are run by other people, with their own rules. Use one you trust, or host your own ([deploy](#deploy)).

## Deploy

Published pages live in `~/nonograph/content`. Tor keys live in `~/nonograph/onion`. **On Nostr** defaults come from `[nostr].relays` and `[nostr].socks` in `Config.toml` (Docker Tor is `127.0.0.1:9050`). Visitors can add extra relays in the browser; those are stored locally and sent to this host only when publishing.

### Docker image (quick)

```bash
mkdir -p ~/nonograph/content ~/nonograph/onion
curl -fsSL -o ~/nonograph/Config.toml \
  https://raw.githubusercontent.com/wuddleko/nonograph/main/Config.toml
# edit ~/nonograph/Config.toml — especially [nostr].relays

sudo docker run -d \
  --name nonograph \
  -p 8009:8009 \
  -v ~/nonograph/content:/app/content \
  -v ~/nonograph/onion:/var/lib/tor/hidden_service \
  -v ~/nonograph/Config.toml:/app/Config.toml:ro \
  --restart unless-stopped \
  ghcr.io/wuddleko/nonograph:latest
```

Clearnet: `http://localhost:8009` (put a reverse proxy in front for HTTPS in production).

Onion address:

```bash
docker logs nonograph 2>&1 | tail -20
# or, once the container is up:
docker exec -u debian-tor nonograph cat /var/lib/tor/hidden_service/hostname
```

### From source (build + Compose)

```bash
git clone https://github.com/wuddleko/nonograph
cd nonograph
# edit Config.toml, then:
make up
```

`make up` uses `docker-compose.yml`, builds the image, and starts container `nonograph_app`. Useful commands: `make logs`, `make status`, `make onion`, `make down`.

### Without Docker

On Debian-style systems: `./scripts/run` (build and run natively). Service helpers: `./scripts/nonograph.sh`, `./scripts/status.sh`.

More detail: [DOCKER.md](DOCKER.md).

## Features
Nonograph comes with an extensive list of markup options; type `/` on a new line to display a list of them.

<img width="561" height="447" alt="image" src="https://github.com/user-attachments/assets/cda96a9c-08bc-4add-bf5b-e8eb0b352201" />


## Screenshots
The homepage and writing area:

<img width="1920" height="1080" alt="homepage" src="https://github.com/user-attachments/assets/d77c065d-a02f-40f5-b29f-fe45465af018" />

The editor with a page in progress:

<img width="1920" height="1080" alt="editor" src="https://github.com/user-attachments/assets/7546a84f-b172-4c0d-a20a-df6c2defbf3c" />

A published page with an image:

<img width="1920" height="1080" alt="published-page2" src="https://github.com/user-attachments/assets/0fc38a43-8bcc-4fbf-9087-ea4100be3e6c" />



## Name
`anonymous` + `monograph` + `telegraph` = `nonograph`

## Audits and security

- 9/5/2026 - [@netqo](https://github.com/netqo) contributed security improvements. Paid in Monero in [this pull request](https://github.com/wuddleko/nonograph/pull/28).
* 8/21/2026 - [@sgpinkus](https://github.com/sgpinkus) contributed security improvements. Paid in Monero in [this pull request](https://github.com/wuddleko/nonograph/pull/26).
* 6/22/2026 - [@SmokeCamel](t.me/cigssss) was reimbursed and provided isolated VMs for running top AI models against Nonograph's parser. No vulnerabilities were found.
* 4/20/2026 - [@h_2_o0](https://t.me/h_2_o0) found a URL validation bypass on 4/20 (nice), fixed in [this commit](https://github.com/wuddleko/nonograph/commit/639f64f010e2b287bf3429af1814dd4fb8697a16).
* 10/15/2025 - [Security Assessment Report Redacted.pdf](https://github.com/user-attachments/files/27242849/Security.Assessment.Report.Redacted.pdf) - audit of the initial release (v0.0.1), paid for in Monero. Only the auditors name and email were redacted. Fixed in [this](https://github.com/wuddleko/nonograph/commit/2641fcaed1aaf458e69217e5489a75c93446b0d2) and [this](https://github.com/wuddleko/nonograph/commit/98178a380324270da704aa80e035aea012e6e748) commit.


## License
Public domain ([Unlicense](https://unlicense.org)). This software belongs to everyone. Use it, modify it, share it without restriction. No attribution required, no strings attached, no warranties provided.
