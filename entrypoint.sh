#!/bin/sh

# Create content directory if it doesn't exist, then fix permissions
mkdir -p /app/content
chown -R nonograph:nonograph /app/content

mkdir -p /var/lib/tor/hidden_service
chown debian-tor:debian-tor /var/lib/tor
chmod 700 /var/lib/tor
chown -R debian-tor:debian-tor /var/lib/tor/hidden_service
chmod 700 /var/lib/tor/hidden_service

# Start Tor as debian-tor user
TOR_LOG=/var/lib/tor/notices.log
: > "$TOR_LOG"
chown debian-tor:debian-tor "$TOR_LOG"
chmod 600 "$TOR_LOG"
mkdir -p /run/tor-control
chown debian-tor:debian-tor /run/tor-control
chmod 750 /run/tor-control
rm -f /run/tor-control/cookie
# setpriv replaces itself, so this pid is Tor. The container has no CAP_KILL,
# so kill cannot probe it. A zombie still has a status file; its state is Z.
setpriv --reuid debian-tor --regid debian-tor --init-groups --inh-caps=-all -- \
    tor -f /etc/tor/torrc \
    --Log "notice file ${TOR_LOG}" \
    --Log "notice stdout" &
TOR_PID=$!

tor_running() {
    state=$(awk '/^State:/ { print $2 }' "/proc/${TOR_PID}/status" 2>/dev/null) || return 1
    [ -n "$state" ] && [ "$state" != "Z" ]
}

COOKIE=/run/tor-control/cookie
i=0
while [ ! -f "$COOKIE" ]; do
    if ! tor_running; then
        echo "Tor exited before the control cookie was ready."
        break
    fi
    sleep 1
    i=$((i + 1))
    if [ $i -ge 30 ]; then
        echo "Tor control cookie did not appear. Circuit paths stay empty."
        break
    fi
done

# Wait for the .onion hostname file to appear
echo "Waiting for Tor hidden service to be ready..."
ONION_FILE="/var/lib/tor/hidden_service/hostname"
i=0
while [ ! -f "$ONION_FILE" ]; do
    if ! tor_running; then
        echo "Tor exited before the hidden service was ready, check logs above."
        break
    fi
    sleep 1
    i=$((i + 1))
    if [ $i -ge 60 ]; then
        echo "Tor hidden service did not start within 60 seconds, check logs above."
        break
    fi
done

if [ -f "$ONION_FILE" ]; then
    ONION=$(cat "$ONION_FILE" 2>/dev/null)
    echo ""
    cat <<'EOF'
    ░   ░░░  ░░░      ░░░   ░░░  ░░░      ░░░░      ░░░       ░░░░      ░░░       ░░░  ░░░░  ░
    ▒    ▒▒  ▒▒  ▒▒▒▒  ▒▒    ▒▒  ▒▒  ▒▒▒▒  ▒▒  ▒▒▒▒▒▒▒▒  ▒▒▒▒  ▒▒  ▒▒▒▒  ▒▒  ▒▒▒▒  ▒▒  ▒▒▒▒  ▒
    ▓  ▓  ▓  ▓▓  ▓▓▓▓  ▓▓  ▓  ▓  ▓▓  ▓▓▓▓  ▓▓  ▓▓▓   ▓▓       ▓▓▓  ▓▓▓▓  ▓▓       ▓▓▓        ▓
    █  ██    ██  ████  ██  ██    ██  ████  ██  ████  ██  ███  ███        ██  ████████  ████  █
    █  ███   ███      ███  ███   ███      ████      ███  ████  ██  ████  ██  ████████  ████  █
                    Write some words, put them on the internet, anonymously.
EOF
    echo ""
    echo "========================================="
    echo "  Your .onion address:"
    echo "  http://$ONION"
    echo "========================================="
    echo ""

    if [ -z "$ONION_URL" ] && [ -n "$ONION" ]; then
        export ONION_URL="http://$ONION"
    fi
fi

echo "Waiting for Tor to bootstrap..."
i=0
while ! grep -q "Bootstrapped 100%" "$TOR_LOG" 2>/dev/null; do
    if ! tor_running; then
        echo "Tor exited before bootstrapping. Starting anyway; relay connections fail until it does."
        break
    fi
    sleep 1
    i=$((i + 1))
    if [ $i -ge 120 ]; then
        echo "Tor did not bootstrap within 120 seconds. Starting anyway; relay connections fail until it does."
        break
    fi
done
if grep -q "Bootstrapped 100%" "$TOR_LOG" 2>/dev/null; then
    echo "Tor is bootstrapped."
fi

# Drop to nonograph user and launch the app. setpriv replaces this process,
# so the app is pid 1 and does not keep the entrypoint's capabilities.
exec setpriv --reuid nonograph --regid nonograph --init-groups --inh-caps=-all -- /app/nonograph
