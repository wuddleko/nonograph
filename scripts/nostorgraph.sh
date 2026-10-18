#!/bin/bash

# Nostorgraph Management Script
# This script helps manage the Nostorgraph service and Tor hidden service

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PID_FILE="$ROOT_DIR/nostorgraph.pid"
BINARY="/usr/local/bin/nostorgraph"
cd "$ROOT_DIR"

pid_is_nostorgraph() {
    local pid="$1"
    local exe
    [ -n "$pid" ] || return 1
    kill -0 "$pid" 2>/dev/null || return 1
    exe=$(readlink "/proc/$pid/exe" 2>/dev/null || true)
    exe=${exe% \(deleted\)}
    [ "$exe" = "$BINARY" ] || [ "$exe" = "$ROOT_DIR/target/release/nostorgraph" ]
}

show_header() {
    echo "╔══════════════════════════════════════╗"
    echo "║         Nostorgraph Manager            ║"
    echo "║    Anonymous Publishing Service      ║"
    echo "╚══════════════════════════════════════╝"
    echo
}

show_help() {
    show_header
    echo "Usage: $0 [command]"
    echo
    echo "Commands:"
    echo "  start       Start Nostorgraph service"
    echo "  stop        Stop Nostorgraph service"
    echo "  restart     Restart Nostorgraph service"
    echo "  status      Show service status"
    echo "  logs        Show recent logs"
    echo "  onion       Show onion address"
    echo "  build       Build the project"
    echo "  clean       Clean build artifacts"
    echo "  tor-start   Start Tor service"
    echo "  tor-stop    Stop Tor service"
    echo "  tor-restart Restart Tor service"
    echo "  help        Show this help message"
    echo
}

start_service() {
    echo "🚀 Starting Nostorgraph..."

    if ! systemctl cat nostorgraph >/dev/null 2>&1; then
        echo "❌ The systemd unit is not installed"
        echo "💡 Install it with: ./scripts/run"
        return 1
    fi

    sudo systemctl start nostorgraph
    local pid=""
    local attempts=0
    while [ $attempts -lt 10 ]; do
        pid=$(systemctl show -p MainPID --value nostorgraph 2>/dev/null || true)
        if pid_is_nostorgraph "$pid"; then
            break
        fi
        sleep 1
        attempts=$((attempts + 1))
    done
    if ! pid_is_nostorgraph "$pid"; then
        echo "❌ Failed to start Nostorgraph"
        echo "📝 Check logs: journalctl -u nostorgraph -n 50"
        return 1
    fi
    echo "$pid" > "$PID_FILE"
    echo "✅ Nostorgraph started successfully"
    echo "🌐 Local access: http://localhost:8009"
}

stop_service() {
    echo "🛑 Stopping Nostorgraph..."

    local pid=""
    if [ -f "$PID_FILE" ]; then
        pid=$(cat "$PID_FILE" 2>/dev/null)
    fi
    if ! pid_is_nostorgraph "$pid" && ! systemctl is-active --quiet nostorgraph 2>/dev/null; then
        echo "⚠️  Nostorgraph is not running"
        rm -f "$PID_FILE"
        return 1
    fi

    sudo systemctl stop nostorgraph 2>/dev/null || true
    if pid_is_nostorgraph "$pid"; then
        kill "$pid" 2>/dev/null || true
        sleep 2
        if pid_is_nostorgraph "$pid"; then
            kill -9 "$pid" 2>/dev/null || true
        fi
    fi
    rm -f "$PID_FILE"
    if pid_is_nostorgraph "$pid"; then
        echo "❌ Failed to stop Nostorgraph"
        return 1
    fi
    echo "✅ Nostorgraph stopped successfully"
}

restart_service() {
    echo "🔄 Restarting Nostorgraph..."
    stop_service
    sleep 1
    start_service
}

show_status() {
    if [ -f "$SCRIPT_DIR/status.sh" ]; then
        "$SCRIPT_DIR/status.sh"
    else
        show_header

        # Check Nostorgraph status
        NOSTORGRAPH_PID=$(cat "$PID_FILE" 2>/dev/null)
        if pid_is_nostorgraph "$NOSTORGRAPH_PID"; then
            echo "✅ Nostorgraph Status: RUNNING (PID: $NOSTORGRAPH_PID)"
        else
            echo "❌ Nostorgraph Status: NOT RUNNING"
        fi

        # Check Tor status
        TOR_STATUS=$(systemctl is-active tor 2>/dev/null)
        if [ "$TOR_STATUS" = "active" ]; then
            echo "✅ Tor Service: RUNNING"
        else
            echo "❌ Tor Service: NOT RUNNING"
        fi

        echo
    fi
}

show_logs() {
    echo "📝 Recent Nostorgraph logs:"
    echo "────────────────────────────────────"
    journalctl -u nostorgraph -n 20 --no-pager 2>/dev/null || echo "No journal entries found"
    echo "────────────────────────────────────"
    echo "💡 Use 'journalctl -u nostorgraph -f' to follow logs in real-time"
}

show_onion() {
    echo "🧅 Tor Hidden Service Information:"
    echo "──────────────────────────────────"

    if sudo test -f "/var/lib/tor/nostorgraph/hostname" 2>/dev/null; then
        ONION_ADDRESS=$(sudo cat /var/lib/tor/nostorgraph/hostname 2>/dev/null)
        if [ -n "$ONION_ADDRESS" ]; then
            echo "Onion Address: http://$ONION_ADDRESS"
            echo
            echo "📋 Copy to clipboard:"
            echo "echo 'http://$ONION_ADDRESS' | xclip -selection clipboard"
            echo
            echo "🌐 Access via Tor Browser:"
            echo "Open Tor Browser and navigate to: http://$ONION_ADDRESS"
        else
            echo "❌ Could not read onion address"
        fi
    else
        echo "❌ Hidden service not configured or not accessible"
        echo "💡 Make sure Tor is running and the hidden service is set up"
    fi
}

build_project() {
    echo "🔨 Building Nostorgraph..."
    cargo build --release --locked
    if [ $? -eq 0 ]; then
        echo "✅ Build completed successfully"
    else
        echo "❌ Build failed"
        return 1
    fi
}

clean_project() {
    echo "🧹 Cleaning build artifacts..."
    cargo clean
    if [ $? -eq 0 ]; then
        echo "✅ Clean completed successfully"
    else
        echo "❌ Clean failed"
        return 1
    fi
}

start_tor() {
    echo "🧅 Starting Tor service..."
    sudo systemctl start tor
    if [ $? -eq 0 ]; then
        echo "✅ Tor service started"
    else
        echo "❌ Failed to start Tor service"
        return 1
    fi
}

stop_tor() {
    echo "🛑 Stopping Tor service..."
    sudo systemctl stop tor
    if [ $? -eq 0 ]; then
        echo "✅ Tor service stopped"
    else
        echo "❌ Failed to stop Tor service"
        return 1
    fi
}

restart_tor() {
    echo "🔄 Restarting Tor service..."
    sudo systemctl restart tor
    if [ $? -eq 0 ]; then
        echo "✅ Tor service restarted"
    else
        echo "❌ Failed to restart Tor service"
        return 1
    fi
}

# Main command handler
case "${1:-help}" in
    "start")
        start_service
        ;;
    "stop")
        stop_service
        ;;
    "restart")
        restart_service
        ;;
    "status")
        show_status
        ;;
    "logs")
        show_logs
        ;;
    "onion")
        show_onion
        ;;
    "build")
        build_project
        ;;
    "clean")
        clean_project
        ;;
    "tor-start")
        start_tor
        ;;
    "tor-stop")
        stop_tor
        ;;
    "tor-restart")
        restart_tor
        ;;
    "help"|*)
        show_help
        ;;
esac
