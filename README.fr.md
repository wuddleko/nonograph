<div align="center">
  <img align="center" width="210" height="120" alt="Nostorgraph" src="logo-proposals/logo-ntor-bw.png" />
</div>

<h1 align="center">Nostorgraph</h1>

<p align="center"><b>Publication anonyme et décentralisée.</b></p>
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

[English](README.md) | [简体中文](README.zh.md) | Français

</div>

Un fork de [nonograph](https://github.com/du82/nonograph) par [du82](https://github.com/du82).

Nostorgraph est une plateforme de publication anonyme et simple, pour qui veut mettre ses mots en ligne sans identité, et sans qu'une seule entité détienne ses pensées.

Écris, clique sur STORE, la note est chiffrée, partage le lien. Quelqu'un ouvre le lien, et son navigateur déchiffre la note.

L'instance stocke la note chiffrée sur des [relais nostr](https://nostr.com/info) via Tor. Ainsi un relai voit du texte chiffré issu d'une sortie Tor.

Pas de compte, pas de profil, pas d'analyse qui suit les lecteurs et les auteurs.

https://github.com/user-attachments/assets/d662c9a2-f0ed-4266-bf55-e2c1f024269e

## Instances

| Disponibilité                                                                        | Lieu          | Clearnet                         | Onion                                                                  |   |
|--------------------------------------------------------------------------------------|---------------|----------------------------------|------------------------------------------------------------------------|---|
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fnonogra.ph)              | 🏴‍☠️ Inconnu   | https://nonogra.ph               | http://ortmy3ey5usdzf4ivht6axtb72owjniaeqrexknosyons544aooltzyd.onion/ |   |
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fwrite.eversiege.network) | 🏴‍☠️ Inconnu   | https://write.eversiege.network/ | http://fmoigm7j3z6vh4hgssdfhlt6knkp443thgxpe5wmbaevvb5km2d3suyd.onion/ |   |
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fnostorgraph.com)           | 🇭🇰 Hong Kong  | https://nostorgraph.com            | http://gt65bmujun7alps7b7oar5x2u5lprxjdvpwrxcrnwkgheocjwllchhqd.onion/ |   |
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fproxy.write.daun.world)  | 🇫🇮 Finlande   | https://proxy.write.daun.world/  | voir ci-dessus                                                         |   |
| Onion                                                                                | 🇷🇺 Russie     |                                  | http://q2w7sdjlmfc5vjif6y3372665wzyqskqvjji7bmhfn72orxcbljvonid.onion/ |   |
| Onion                                                                                | 🇭🇺 Hongrie    |                                  | http://t7fgh7qvjysh3wer747m6dkjvkjsqvajyv5bh2grzjgpd2derxsxbdad.onion/ |   |
| Onion                                                                                | 🏴‍☠️ Inconnu   |                                  | http://uawaa47jvsfr3ij63ns25xp6qvhqswsx3fgij2evbrcnt3ygxq3dbwyd.onion/ |   |
| Onion                                                                                | 🇰🇿 Kazakhstan |                                  | http://5mq3db45agipsceghnpx3iumlctya3absmp4sgnitqcmrmhaqhbbjcid.onion/ |   |

Ces instances sont tenues par d'autres personnes, avec leurs propres règles. Utilisez-en une en qui vous avez confiance, ou hébergez la vôtre ([déployer](#déployer)).

## Déployer

`~/nostorgraph/content` contient des copies facultatives des quatre pages du site (à propos, mentions légales, api, balisage) et le pool de relais exploré, `relay-pool.json`. Les notes publiées n'y sont pas stockées. Les clés Tor sont dans `~/nostorgraph/onion`. L'état Tor, y compris le garde d'entrée, est dans `~/nostorgraph/tor`. Le trafic sortant vers les relais utilise `[nostr].socks` dans `Config.toml` (le Tor de Docker est `127.0.0.1:9050`). `[nostr].relays` nomme les relais absents du lien de partage.

### Image Docker (rapide)

```bash
mkdir -p ~/nostorgraph/content ~/nostorgraph/tor ~/nostorgraph/onion
curl -fsSL -o ~/nostorgraph/Config.toml \
  https://raw.githubusercontent.com/wuddleko/nostorgraph/main/Config.toml
# modifier ~/nostorgraph/Config.toml — [nostr].socks doit être le port Tor local

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

Clearnet : `http://localhost:8009` (placez un reverse proxy devant pour le HTTPS en production). Le port publié n'écoute que sur la boucle locale de l'hôte.

Adresse onion :

```bash
docker logs nostorgraph 2>&1 | tail -20
# ou, une fois le conteneur démarré :
docker exec -u debian-tor nostorgraph cat /var/lib/tor/hidden_service/hostname
```

### Depuis les sources (compilation + Compose)

```bash
git clone https://github.com/wuddleko/nostorgraph
cd nostorgraph
# modifier Config.toml, puis :
make up
```

`make up` utilise `docker-compose.yml`, construit l'image et démarre le conteneur `nostorgraph_app`. Commandes utiles : `make logs`, `make status`, `make onion`, `make down`.

### Sans Docker

Sur un système de type Debian, `./scripts/run` compile le binaire et installe un service systemd qui tourne sous l'utilisateur `nostorgraph`. Aides du service : `./scripts/nostorgraph.sh`, `./scripts/status.sh`.

Plus de détails : [DOCKER.md](DOCKER.md).

## Fonctionnalités
Nostorgraph propose une longue liste d'options de balisage ; tapez `/` pour les afficher.

<img width="561" height="447" alt="image" src="https://github.com/user-attachments/assets/cda96a9c-08bc-4add-bf5b-e8eb0b352201" />


## Captures d'écran
La page d'accueil et la zone d'écriture :

<img width="1920" height="1080" alt="homepage" src="https://github.com/user-attachments/assets/d77c065d-a02f-40f5-b29f-fe45465af018" />

L'éditeur avec une page en cours :

<img width="1920" height="1080" alt="editor" src="https://github.com/user-attachments/assets/7546a84f-b172-4c0d-a20a-df6c2defbf3c" />

Une page publiée avec une image :

<img width="1920" height="1080" alt="published-page2" src="https://github.com/user-attachments/assets/0fc38a43-8bcc-4fbf-9087-ea4100be3e6c" />



## Nom
`anonymous` + `monograph` + `telegraph` = `nonograph`  

`nonograph` + `tor` + `nostr` = `nostorgraph`

## Licence
Domaine public ([Unlicense](https://unlicense.org)). Ce logiciel appartient à tout le monde. Utilisez-le, modifiez-le, partagez-le sans restriction. Pas d'attribution exigée, pas de conditions cachées, aucune garantie fournie.
