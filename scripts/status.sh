#!/bin/bash

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "╔══════════════════════════════════════╗"
echo "║         Nonograph Status             ║"
echo "║    Anonymous Publishing Service      ║"
echo "╚══════════════════════════════════════╝"
echo

# Check if Nonograph is running
PID_FILE="$ROOT_DIR/nonograph.pid"
NONOGRAPH_PID=$(cat "$PID_FILE" 2>/dev/null)
NONOGRAPH_EXE=$(readlink "/proc/$NONOGRAPH_PID/exe" 2>/dev/null || true)
NONOGRAPH_EXE=${NONOGRAPH_EXE% \(deleted\)}
if [ -n "$NONOGRAPH_PID" ] && kill -0 "$NONOGRAPH_PID" 2>/dev/null && { [ "$NONOGRAPH_EXE" = "/usr/local/bin/nonograph" ] || [ "$NONOGRAPH_EXE" = "$ROOT_DIR/target/release/nonograph" ]; }; then
    echo "✅ Nonograph Status: RUNNING (PID: $NONOGRAPH_PID)"
    echo "🌐 Local Access: http://localhost:8009"
else
    echo "❌ Nonograph Status: NOT RUNNING"
fi

echo

# Check Tor service status
TOR_STATUS=$(systemctl is-active tor 2>/dev/null)
if [ "$TOR_STATUS" = "active" ]; then
    echo "✅ Tor Service: RUNNING"

    # Check if hidden service directory exists
    if sudo test -d "/var/lib/tor/nonograph" 2>/dev/null; then
        echo "✅ Hidden Service: CONFIGURED"

        # Get onion address
        if sudo test -f "/var/lib/tor/nonograph/hostname" 2>/dev/null; then
            ONION_ADDRESS=$(sudo cat /var/lib/tor/nonograph/hostname 2>/dev/null)
            if [ -n "$ONION_ADDRESS" ]; then
                echo "🧅 Onion Address: http://$ONION_ADDRESS"
                echo "📋 Copy command: echo 'http://$ONION_ADDRESS' | xclip -selection clipboard"
            else
                echo "⚠️  Onion Address: Not yet generated"
            fi
        else
            echo "⚠️  Onion Address: Hostname file not found"
        fi
    else
        echo "❌ Hidden Service: NOT CONFIGURED"
    fi
else
    echo "❌ Tor Service: NOT RUNNING"
fi

echo

# Show recent log entries
echo "📝 Recent Log Entries:"
journalctl -u nonograph -n 5 --no-pager 2>/dev/null | sed 's/^/   /' || echo "   No journal entries found"

echo

# Show system resources
echo "💻 System Resources:"
echo "   Memory Usage: $(free -h | awk '/^Mem:/ {print $3 "/" $2}')"
echo "   Disk Usage: $(df -h "$ROOT_DIR" | awk 'NR==2 {print $3 "/" $2 " (" $5 " used)"}')"

echo

# Show network connectivity
echo "🌍 Network Status:"
if curl -s --max-time 5 http://localhost:8009 > /dev/null 2>&1; then
    echo "   ✅ Local service accessible"
else
    echo "   ❌ Local service not accessible"
fi

echo

echo "Commands:"
echo "  Start Nonograph: sudo systemctl start nonograph"
echo "  Stop Nonograph:  sudo systemctl stop nonograph"
echo "  Start Tor:          sudo systemctl start tor"
echo "  Stop Tor:           sudo systemctl stop tor"
echo "  View logs:          tail -f $ROOT_DIR/nonograph.log"
