#!/usr/bin/env bash
# BeeBotOS Production Runner
# Usage: ./beebotos-run.sh [gateway|web|beehub]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

# Ensure data directories exist
mkdir -p data/run data/workspace data/logs

run_gateway() {
    echo "Starting Gateway on port 8000..."
    nohup ./beebotos-gateway > data/logs/gateway.log 2>&1 &
    echo $! > data/run/gateway.pid
    sleep 1
    if kill -0 "$(cat data/run/gateway.pid)" 2>/dev/null; then
        echo "Gateway started (PID: $(cat data/run/gateway.pid))"
    else
        echo "Gateway failed to start. Check data/logs/gateway.log"
    fi
}

run_web() {
    echo "Starting Web Server on port 8090..."
    nohup ./web-server > data/logs/web.log 2>&1 &
    echo $! > data/run/web.pid
    sleep 1
    if kill -0 "$(cat data/run/web.pid)" 2>/dev/null; then
        echo "Web Server started (PID: $(cat data/run/web.pid))"
    else
        echo "Web Server failed to start. Check data/logs/web.log"
    fi
}

run_beehub() {
    if [[ ! -f ./beehub ]]; then
        echo "BeeHub binary not found, skipping."
        return 0
    fi
    echo "Starting BeeHub on port 8080..."
    nohup ./beehub > data/logs/beehub.log 2>&1 &
    echo $! > data/run/beehub.pid
    sleep 1
    if kill -0 "$(cat data/run/beehub.pid)" 2>/dev/null; then
        echo "BeeHub started (PID: $(cat data/run/beehub.pid))"
    else
        echo "BeeHub failed to start. Check data/logs/beehub.log"
    fi
}

case "${1:-all}" in
    gateway) run_gateway ;;
    web) run_web ;;
    beehub) run_beehub ;;
    all)
        run_gateway
        run_web
        run_beehub
        ;;
    *)
        echo "Usage: $0 [gateway|web|beehub|all]"
        exit 1
        ;;
esac
