FROM rust:latest AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown

WORKDIR /app

COPY Cargo.toml build.rs robots.txt ./
COPY parser ./parser
COPY page ./page
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim

LABEL org.opencontainers.image.source=https://github.com/wuddleko/nonograph
LABEL org.opencontainers.image.description="Anonymous publishing for the privacy-conscious web"
LABEL org.opencontainers.image.licenses=Unlicense

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    tor \
    sudo \
    && rm -rf /var/lib/apt/lists/*

RUN echo "DataDirectory /var/lib/tor" > /etc/tor/torrc && \
    echo "SocksPort 127.0.0.1:9050 IsolateSOCKSAuth" >> /etc/tor/torrc && \
    echo "SocksPolicy accept 127.0.0.1" >> /etc/tor/torrc && \
    echo "SocksPolicy reject *" >> /etc/tor/torrc && \
    echo "ControlPort 127.0.0.1:9051" >> /etc/tor/torrc && \
    echo "CookieAuthentication 1" >> /etc/tor/torrc && \
    echo "CookieAuthFile /tmp/tor-control-cookie" >> /etc/tor/torrc && \
    echo "CookieAuthFileGroupReadable 1" >> /etc/tor/torrc && \
    echo "ControlSocket 0" >> /etc/tor/torrc && \
    echo "" >> /etc/tor/torrc && \
    echo "HiddenServiceDir /var/lib/tor/hidden_service/" >> /etc/tor/torrc && \
    echo "HiddenServicePort 80 127.0.0.1:8009" >> /etc/tor/torrc

RUN useradd -r -s /bin/false -u 1000 nonograph && \
    usermod -aG debian-tor nonograph
RUN mkdir -p /app/content /app/templates /var/lib/tor && \
    mkdir -p /var/lib/tor/hidden_service && \
    chmod 700 /var/lib/tor && \
    chmod 700 /var/lib/tor/hidden_service && \
    chown -R nonograph:nonograph /app && \
    chown -R debian-tor:debian-tor /var/lib/tor && \
    echo "nonograph ALL=(debian-tor) NOPASSWD: /usr/bin/tor" >> /etc/sudoers && \
    echo "root ALL=(nonograph) NOPASSWD: /app/nonograph" >> /etc/sudoers

COPY --from=builder /app/target/release/nonograph /app/nonograph
COPY Config.toml /app/Config.toml
COPY templates/ /app/templates/
COPY pages/ /app/pages/
COPY entrypoint.sh /app/entrypoint.sh

RUN sed -i 's/address = "127.0.0.1"/address = "0.0.0.0"/' /app/Config.toml || true && \
    chmod +x /app/entrypoint.sh && \
    chown -R nonograph:nonograph /app/pages /app/templates /app/Config.toml /app/nonograph

WORKDIR /app
EXPOSE 8009

ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=8009

HEALTHCHECK --interval=30s --timeout=3s --start-period=240s --retries=3 \
    CMD curl -f http://localhost:8009/ || exit 1

CMD ["/app/entrypoint.sh"]
