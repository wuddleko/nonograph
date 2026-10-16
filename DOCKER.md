# Docker Setup

## Install Docker

**Linux:**
```bash
curl -fsSL https://get.docker.com -o get-docker.sh
sh get-docker.sh
sudo usermod -aG docker $USER
```

**Windows:**
Download and install [Docker Desktop](https://www.docker.com/products/docker-desktop/).

## Run with Docker Compose (recommended)

```bash
make up        # Start
make tor       # Get .onion address
make status    # Check status
make down      # Stop
make clean     # Remove this project's containers, images, and volumes
```

`~/nonograph/content` holds optional copies of the about, legal, markup, and api pages. Those four are copied there on startup when they are missing. Published notes are not stored in this directory. `~/nonograph/tor` holds Tor state, including the entry guard. `~/nonograph/onion` holds the hidden-service keys.

Access: http://localhost:8009 (the published port listens on the host loopback only).

## Run manually

**Linux/macOS:**
```bash
mkdir -p ~/nonograph/content ~/nonograph/tor ~/nonograph/onion
docker run -d \
  --name nonograph \
  --read-only \
  --cap-drop ALL \
  --cap-add SETUID --cap-add SETGID --cap-add CHOWN \
  --cap-add DAC_OVERRIDE --cap-add FOWNER \
  --security-opt no-new-privileges:true \
  --tmpfs /tmp --tmpfs /run \
  -p 127.0.0.1:8009:8009 \
  -v ~/nonograph/content:/app/content \
  -v ~/nonograph/tor:/var/lib/tor \
  -v ~/nonograph/onion:/var/lib/tor/hidden_service \
  --restart unless-stopped \
  ghcr.io/wuddleko/nonograph:latest
```

**Windows (PowerShell):**
```powershell
mkdir -Force $env:USERPROFILE\nonograph\content, $env:USERPROFILE\nonograph\tor, $env:USERPROFILE\nonograph\onion
docker run -d `
  --name nonograph `
  --read-only `
  --cap-drop ALL `
  --cap-add SETUID --cap-add SETGID --cap-add CHOWN `
  --cap-add DAC_OVERRIDE --cap-add FOWNER `
  --security-opt no-new-privileges:true `
  --tmpfs /tmp --tmpfs /run `
  -p 127.0.0.1:8009:8009 `
  -v $env:USERPROFILE\nonograph\content:/app/content `
  -v $env:USERPROFILE\nonograph\tor:/var/lib/tor `
  -v $env:USERPROFILE\nonograph\onion:/var/lib/tor/hidden_service `
  --restart unless-stopped `
  ghcr.io/wuddleko/nonograph:latest
```

Then check logs for your `.onion` address:

```bash
docker logs nonograph
```
