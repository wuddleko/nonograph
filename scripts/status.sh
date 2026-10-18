#!/bin/bash

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "╔══════════════════════════════════════╗"
echo "║         Nostorgraph Status             ║"
echo "║    Anonymous Publishing Service      ║"
echo "╚══════════════════════════════════════╝"
echo

# Check if Nostorgraph is running
PID_FILE="$ROOT_DIR/nostorgraph.pid"
NOSTORGRAPH_PID=$(cat "$PID_FILE" 2>/dev/null)
NOSTORGRAPH_EXE=$(readlink "/proc/$NOSTORGRAPH_PID/exe" 2>/dev/null || true)
NOSTORGRAPH_EXE=${NOSTORGRAPH_EXE% \(deleted\)}
if [ -n "$NOSTORGRAPH_PID" ] && kill -0 "$NOSTORGRAPH_PID" 2>/dev/null && { [ "$NOSTORGRAPH_EXE" = "/usr/local/bin/nostorgraph" ] || [ "$NOSTORGRAPH_EXE" = "$ROOT_DIR/target/release/nostorgraph" ]; }; then
    echo "✅ Nostorgraph Status: RUNNING (PID: $NOSTORGRAPH_PID)"
    echo "🌐 Local Access: http://localhost:8009"
else
    echo "❌ Nostorgraph Status: NOT RUNNING"
fi

echo

# Check Tor service status
TOR_STATUS=$(systemctl is-active tor 2>/dev/null)
if [ "$TOR_STATUS" = "active" ]; then
    echo "✅ Tor Service: RUNNING"

    # Check if hidden service directory exists
    if sudo test -d "/var/lib/tor/nostorgraph" 2>/dev/null; then
        echo "✅ Hidden Service: CONFIGURED"

        # Get onion address
        if sudo test -f "/var/lib/tor/nostorgraph/hostname" 2>/dev/null; then
            ONION_ADDRESS=$(sudo cat /var/lib/tor/nostorgraph/hostname 2>/dev/null)
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
journalctl -u nostorgraph -n 5 --no-pager 2>/dev/null | sed 's/^/   /' || echo "   No journal entries found"

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
echo "  Start Nostorgraph: sudo systemctl start nostorgraph"
echo "  Stop Nostorgraph:  sudo systemctl stop nostorgraph"
echo "  Start Tor:          sudo systemctl start tor"
echo "  Stop Tor:           sudo systemctl stop tor"
echo "  View logs:          tail -f $ROOT_DIR/nostorgraph.log"
