FROM rust:latest@sha256:e259494b14672950a909b4c54527e19d29cae6ebf980672114c8bb85e5594e4a AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown

WORKDIR /app

COPY Cargo.toml Cargo.lock build.rs robots.txt ./
COPY logo-proposals/favicon-nt-bw.png ./logo-proposals/favicon-nt-bw.png
COPY parser ./parser
COPY page ./page
COPY nip44 ./nip44
COPY crawler ./crawler
COPY pdf ./pdf
COPY src ./src

RUN cargo build --release --locked

FROM debian:bookworm-slim@sha256:7c7b2c966bc9ee8cedfeef67e0e279108992c77681fa595db4a9d65c06ccc587

LABEL org.opencontainers.image.source=https://github.com/wuddleko/nostorgraph
LABEL org.opencontainers.image.description="Anonymous publishing for the privacy-conscious web"
LABEL org.opencontainers.image.licenses=Unlicense

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    tor \
    && rm -rf /var/lib/apt/lists/*

# Cookie auth is full Tor control (SETCONF, SIGNAL SHUTDOWN), not GETINFO
# circuit-status. debian-tor group membership is the grant that lets
# nostorgraph read the cookie. /run/tor-control is not writable by nostorgraph.
RUN echo "DataDirectory /var/lib/tor" > /etc/tor/torrc && \
    echo "SocksPort 127.0.0.1:9050 IsolateSOCKSAuth" >> /etc/tor/torrc && \
    echo "SocksPolicy accept 127.0.0.1" >> /etc/tor/torrc && \
    echo "SocksPolicy reject *" >> /etc/tor/torrc && \
    echo "ControlPort 127.0.0.1:9051" >> /etc/tor/torrc && \
    echo "CookieAuthentication 1" >> /etc/tor/torrc && \
    echo "CookieAuthFile /run/tor-control/cookie" >> /etc/tor/torrc && \
    echo "CookieAuthFileGroupReadable 1" >> /etc/tor/torrc && \
    echo "ControlSocket 0" >> /etc/tor/torrc && \
    echo "" >> /etc/tor/torrc && \
    echo "HiddenServiceDir /var/lib/tor/hidden_service/" >> /etc/tor/torrc && \
    echo "HiddenServicePort 80 127.0.0.1:8009" >> /etc/tor/torrc && \
    echo "HiddenServiceEnableIntroDoSDefense 1" >> /etc/tor/torrc && \
    echo "HiddenServiceEnableIntroDoSRatePerSec 25" >> /etc/tor/torrc && \
    echo "HiddenServiceEnableIntroDoSBurstPerSec 200" >> /etc/tor/torrc && \
    if tor --list-torrc-options 2>/dev/null | grep -q HiddenServicePoWDefensesEnabled; then \
      echo "HiddenServicePoWDefensesEnabled 1" >> /etc/tor/torrc; \
    fi

RUN useradd -r -s /bin/false -u 1000 nostorgraph && \
    usermod -aG debian-tor nostorgraph
RUN mkdir -p /app/content /app/templates /var/lib/tor /run/tor-control && \
    mkdir -p /var/lib/tor/hidden_service && \
    chmod 700 /var/lib/tor && \
    chmod 700 /var/lib/tor/hidden_service && \
    chown -R nostorgraph:nostorgraph /app && \
    chown -R debian-tor:debian-tor /var/lib/tor && \
    chown debian-tor:debian-tor /run/tor-control && \
    chmod 750 /run/tor-control

COPY --from=builder /app/target/release/nostorgraph /app/nostorgraph
COPY Config.toml /app/Config.toml
COPY templates/ /app/templates/
COPY pages/ /app/pages/
COPY entrypoint.sh /app/entrypoint.sh

RUN sed -i 's/address = "127.0.0.1"/address = "0.0.0.0"/' /app/Config.toml || true && \
    chmod +x /app/entrypoint.sh && \
    chown -R nostorgraph:nostorgraph /app/pages /app/templates /app/Config.toml /app/nostorgraph

WORKDIR /app
EXPOSE 8009

ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=8009

HEALTHCHECK --interval=30s --timeout=3s --start-period=240s --retries=3 \
    CMD curl -f http://localhost:8009/ || exit 1

CMD ["/app/entrypoint.sh"]
