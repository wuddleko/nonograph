
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
  <a href="http://cpvqcbj7effj5d7hquxck2vek2thkhhgi2w3y3776ixbjn3lbvjoufad.onion/">
    <img src="https://img.shields.io/badge/Tor-Hidden%20Service-7d4698?style=flat&logo=torproject&logoColor=white" alt="Tor Hidden Service">
  </a>

English | [简体中文](README.zh.md) | [Français](README.fr.md)

</div>

A fork of [nonograph](https://github.com/du82/nonograph) by [du82](https://github.com/du82).

Nostorgraph is a simple anonymous publishing platform for anyone who wants their words online without an identity, and without a single entity holding their thoughts.  

You write, you click STORE, the note is encrypted, and you get a link. Someone opens that link, and their browser decrypts it.  

The instance stores the encrypted note on [nostr relays](https://nostr.com/info) through Tor. Thus a relay sees ciphertext from a Tor exit.

No account, no profile, no analytics following readers and writers around.  

https://github.com/user-attachments/assets/d1658c7b-da62-4a6d-8701-36dae6517097

## Instances

| Uptime                                                                  | Location    | Clearnet              | Onion                                                                                   |
|-------------------------------------------------------------------------|-------------|-----------------------|-----------------------------------------------------------------------------------------|
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fnostorgra.ph) | 🇩🇪 Germany | https://nostorgra.ph | http://cpvqcbj7effj5d7hquxck2vek2thkhhgi2w3y3776ixbjn3lbvjoufad.onion/ |

Host your own if you want another ([deploy](#deploy)).

## Deploy

```bash
git clone https://github.com/wuddleko/nostorgraph
cd nostorgraph
make up
```

Open http://localhost:8009. That port listens on the host loopback only. Put a reverse proxy in front for HTTPS.

`make up` builds the image and starts container `nostorgraph_app`. `make onion` prints the onion address. Also: `make logs`, `make status`, `make down`.

Edit `Config.toml` before `make up` when the Tor port or the relay list should differ from the defaults. `[nostr].socks` is the local Tor port. Docker uses `127.0.0.1:9050`.

State stays in your home directory:

- `~/nostorgraph/content` holds optional copies of the about, legal, api, and markup pages, and the crawled relay pool. Published notes are not stored there.
- `~/nostorgraph/onion` holds the Tor hidden-service keys.
- `~/nostorgraph/tor` holds Tor state, including the entry guard.

The prebuilt image is `ghcr.io/wuddleko/nostorgraph:latest`. On Debian-style systems without Docker, `./scripts/run` builds the binary and installs a systemd service. More detail: [DOCKER.md](DOCKER.md).

## Name
`anonymous` + `monograph` + `telegraph` = `nonograph`  

`nonograph` + `tor` + `nostr` = `nostorgraph`

## License
Public domain ([Unlicense](https://unlicense.org)). This software belongs to everyone. Use it, modify it, share it without restriction. No attribution required, no strings attached, no warranties provided.
