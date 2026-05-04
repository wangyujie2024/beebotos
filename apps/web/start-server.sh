#!/bin/bash
# Start BeeBotOS Web Server (Rust Implementation) on port 8090

PORT=8090
HOST=0.0.0.0
# 自动发现脚本所在目录，避免写死绝对路径
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
STATIC_PATH="$SCRIPT_DIR"
GATEWAY_URL="http://localhost:8000"

# 检查端口是否被占用
PID=$(lsof -ti:$PORT 2>/dev/null)
if [ -n "$PID" ]; then
    echo "Port $PORT is occupied by PID $PID, killing..."
    kill -9 $PID 2>/dev/null
    sleep 1
fi

# 检查静态文件目录
if [ ! -d "$STATIC_PATH" ]; then
    echo "Error: Static file directory '$STATIC_PATH' not found!"
    echo "Please run 'wasm-pack build --target web --out-dir pkg' first"
    exit 1
fi

# 检查 index.html
if [ ! -f "$STATIC_PATH/index.html" ]; then
    echo "Warning: index.html not found in $STATIC_PATH"
fi

# 获取本机IP
LOCAL_IP=$(hostname -I | awk '{print $1}')

echo "Starting BeeBotOS Web Server (Rust)..."
echo "  - Port: $PORT"
echo "  - Host: $HOST"
echo "  - Static: $STATIC_PATH"
echo "  - Gateway: $GATEWAY_URL"
echo ""

# 切换到脚本所在目录并启动 Rust 服务器
cd "$SCRIPT_DIR"
exec cargo run --bin web-server --features server -- \
    --host $HOST \
    --port $PORT \
    --static-path "$STATIC_PATH" \
    --gateway-url "$GATEWAY_URL" \
    --log-level info
