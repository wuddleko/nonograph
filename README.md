<div align="center">
  <img align="center" width="210" height="120" alt="Nostorgraph" src="logo-proposals/logo-ntor-bw.png" />
</div>

<h1 align="center">Nostorgraph</h1>

<p align="center"><b>Anonymous and decentralized publishing for the shared-privacy-conscious web.</b></p>
<div align="center">
  <a href="https://unlicense.org">
    <img alt="GitHub License" src="https://img.shields.io/github/license/wuddleko/nostorgraph">
  </a>
  <a href="https://github.com/wuddleko/nostorgraph/releases/latest">
    <img alt="GitHub Release" src="https://img.shields.io/github/v/release/wuddleko/nostorgraph">
  </a>
  <a href="https://github.com/wuddleko/nostorgraph/commits/main/">
    <img alt="GitHub commit activity" src="https://img.shields.io/github/commit-activity/m/wuddleko/nostorgraph">
  </a>
  <a href="http://ortmy3ey5usdzf4ivht6axtb72owjniaeqrexknosyons544aooltzyd.onion/">
    <img src="https://img.shields.io/badge/Tor-Hidden%20Service-7d4698?style=flat&logo=torproject&logoColor=white" alt="Tor Hidden Service">
  </a>

English | [简体中文](README.zh.md) | [Français](README.fr.md)

</div>

A fork of [nonograph](https://github.com/du82/nonograph) by [du82](https://github.com/du82).

Nostorgraph is a simple anonymous publishing platform for anyone who wants their words online without an identity, and without a single entity holding their thoughts.  

You write, you click STORE, the note is encrypted, and you get a link. Someone opens that link, and their browser decrypts it.  

The instance stores the encrypted note on [nostr relays](https://nostr.com/info) through Tor. Thus a relay sees ciphertext from a Tor exit.

No account, no profile, no analytics following readers and writers around.  

https://github.com/user-attachments/assets/d662c9a2-f0ed-4266-bf55-e2c1f024269e

## Instances

| Uptime                                                                               | Location      | Clearnet                         | Onion                                                                  |   |
|--------------------------------------------------------------------------------------|---------------|----------------------------------|------------------------------------------------------------------------|---|
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fnonogra.ph)              | 🏴‍☠️ Unknown    | https://nonogra.ph               | http://ortmy3ey5usdzf4ivht6axtb72owjniaeqrexknosyons544aooltzyd.onion/ |   |
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fwrite.eversiege.network) | 🏴‍☠️ Unknown    | https://write.eversiege.network/ | http://fmoigm7j3z6vh4hgssdfhlt6knkp443thgxpe5wmbaevvb5km2d3suyd.onion/ |   |
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fnostorgraph.com)           | 🇭🇰 Hong Kong  | https://nostorgraph.com            | http://gt65bmujun7alps7b7oar5x2u5lprxjdvpwrxcrnwkgheocjwllchhqd.onion/ |   |
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fproxy.write.daun.world)  | 🇫🇮 Finland    | https://proxy.write.daun.world/  | see above                                                              |   |
| Onion                                                                                | 🇷🇺 Russia     |                                  | http://q2w7sdjlmfc5vjif6y3372665wzyqskqvjji7bmhfn72orxcbljvonid.onion/ |   |
| Onion                                                                                | 🇭🇺 Hungary    |                                  | http://t7fgh7qvjysh3wer747m6dkjvkjsqvajyv5bh2grzjgpd2derxsxbdad.onion/ |   |
| Onion                                                                                | 🏴‍☠️ Unknown    |                                  | http://uawaa47jvsfr3ij63ns25xp6qvhqswsx3fgij2evbrcnt3ygxq3dbwyd.onion/ |   |
| Onion                                                                                | 🇰🇿 Kazakhstan |                                  | http://5mq3db45agipsceghnpx3iumlctya3absmp4sgnitqcmrmhaqhbbjcid.onion/ |   |

These are run by other people, with their own rules. Use one you trust, or host your own ([deploy](#deploy)).

## Deploy

`~/nostorgraph/content` holds optional copies of the four site pages (about, legal, api, markup) and the crawled relay pool, `relay-pool.json`. Published notes are not stored there. Tor keys live in `~/nostorgraph/onion`. Tor state, including the entry guard, lives in `~/nostorgraph/tor`. Outbound relay traffic uses `[nostr].socks` in `Config.toml` (Docker Tor is `127.0.0.1:9050`). `[nostr].relays` names the relays left off the share link.

### Docker image (quick)

```bash
mkdir -p ~/nostorgraph/content ~/nostorgraph/tor ~/nostorgraph/onion
curl -fsSL -o ~/nostorgraph/Config.toml \
  https://raw.githubusercontent.com/wuddleko/nostorgraph/main/Config.toml
# edit ~/nostorgraph/Config.toml — [nostr].socks should be the local Tor port

sudo docker run -d \
  --name nostorgraph \
  --read-only \
  --cap-drop ALL \
  --cap-add SETUID --cap-add SETGID --cap-add CHOWN \
  --cap-add DAC_OVERRIDE --cap-add FOWNER \
  --security-opt no-new-privileges:true \
  --tmpfs /tmp --tmpfs /run \
  -p 127.0.0.1:8009:8009 \
  -v ~/nostorgraph/content:/app/content \
  -v ~/nostorgraph/tor:/var/lib/tor \
  -v ~/nostorgraph/onion:/var/lib/tor/hidden_service \
  -v ~/nostorgraph/Config.toml:/app/Config.toml:ro \
  --restart unless-stopped \
  ghcr.io/wuddleko/nostorgraph:latest
```

Clearnet: `http://localhost:8009` (put a reverse proxy in front for HTTPS in production). The published port listens on the host loopback only.

Onion address:

```bash
docker logs nostorgraph 2>&1 | tail -20
# or, once the container is up:
docker exec -u debian-tor nostorgraph cat /var/lib/tor/hidden_service/hostname
```

### From source (build + Compose)

```bash
git clone https://github.com/wuddleko/nostorgraph
cd nostorgraph
# edit Config.toml, then:
make up
```

`make up` uses `docker-compose.yml`, builds the image, and starts container `nostorgraph_app`. Useful commands: `make logs`, `make status`, `make onion`, `make down`.

### Without Docker

On Debian-style systems: `./scripts/run` builds the binary and installs a systemd service that runs as the `nostorgraph` user. Service helpers: `./scripts/nostorgraph.sh`, `./scripts/status.sh`.

More detail: [DOCKER.md](DOCKER.md).

## Features
Nostorgraph comes with an extensive list of markup options; type `/` to display a list of them.

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

`nonograph` + `tor` + `nostr` = `nostorgraph`

## License
Public domain ([Unlicense](https://unlicense.org)). This software belongs to everyone. Use it, modify it, share it without restriction. No attribution required, no strings attached, no warranties provided.
