#!/usr/bin/env bash
# BeeBotOS Development Manager (Linux/macOS)
# Usage: ./scripts/beebotos-dev.sh [menu|build|start|stop|restart|run|pack|status] [service|all]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
PID_DIR="${PROJECT_ROOT}/data/run"
mkdir -p "${PID_DIR}"

cd "${PROJECT_ROOT}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

print_header() {
    echo -e "${CYAN}========================================${NC}"
    echo -e "${CYAN}  BeeBotOS Development Manager${NC}"
    echo -e "${CYAN}========================================${NC}"
    echo ""
}

print_error()   { echo -e "${RED}[ERROR]${NC} $1"; }
print_info()    { echo -e "${BLUE}[INFO]${NC} $1"; }
print_success() { echo -e "${GREEN}[OK]${NC} $1"; }
print_warn()    { echo -e "${YELLOW}[WARN]${NC} $1"; }

# Service definitions
# Format: name|build_cmd|binary_path|port|description
SERVICES=(
    "gateway|cargo build --release -p beebotos-gateway|target/release/beebotos-gateway|8000|API Gateway"
    "web|cargo build --release --lib -p beebotos-web --target wasm32-unknown-unknown && wasm-pack build --target web --out-dir pkg apps/web/ && cargo build --release --bin web-server|target/release/web-server|8090|Web Frontend Server"
    "beehub|cargo build --release -p beebotos-beehub|target/release/beehub|8080|BeeHub Service"
    "cli|cargo install --path apps/cli --force|||CLI Tool (install only)"
)

get_service_field() {
    local svc="$1"
    local idx="$2"
    for entry in "${SERVICES[@]}"; do
        IFS='|' read -r name build_cmd binary port desc <<< "$entry"
        if [[ "$name" == "$svc" ]]; then
            case $idx in
                1) echo "$build_cmd" ;;
                2) echo "$binary" ;;
                3) echo "$port" ;;
                4) echo "$desc" ;;
            esac
            return
        fi
    done
}

service_names() {
    local names=()
    for entry in "${SERVICES[@]}"; do
        IFS='|' read -r name _ _ _ _ <<< "$entry"
        names+=("$name")
    done
    echo "${names[@]}"
}

is_valid_service() {
    local target="$1"
    for name in $(service_names); do
        [[ "$name" == "$target" ]] && return 0
    done
    return 1
}

build_service() {
    local svc="$1"
    local cmd=$(get_service_field "$svc" 1)
    local desc=$(get_service_field "$svc" 4)

    echo -e "${CYAN}----------------------------------------${NC}"
    echo -e "${CYAN}Building: ${desc} (${svc})${NC}"
    echo -e "${CYAN}----------------------------------------${NC}"

    if [[ -z "$cmd" ]]; then
        print_warn "No build command for ${svc}, skipping."
        return 0
    fi

    if eval "$cmd"; then
        print_success "Build completed: ${svc}"
        return 0
    else
        print_error "Build failed: ${svc}"
        return 1
    fi
}

get_pid_file() {
    echo "${PID_DIR}/${1}.pid"
}

is_running() {
    local svc="$1"
    local pid_file=$(get_pid_file "$svc")
    if [[ -f "$pid_file" ]]; then
        local pid=$(cat "$pid_file")
        if kill -0 "$pid" 2>/dev/null; then
            return 0
        fi
    fi
    return 1
}

start_service() {
    local svc="$1"
    local binary=$(get_service_field "$svc" 2)
    local port=$(get_service_field "$svc" 3)
    local desc=$(get_service_field "$svc" 4)
    local pid_file=$(get_pid_file "$svc")

    if [[ -z "$binary" ]]; then
        print_warn "${svc} is not a daemon service, skipping start."
        return 0
    fi

    if is_running "$svc"; then
        print_warn "${svc} is already running (PID: $(cat "$pid_file"))"
        return 0
    fi

    if [[ ! -f "$binary" ]]; then
        print_error "Binary not found: $binary"
        print_info "Please build ${svc} first."
        return 1
    fi

    echo -e "${CYAN}Starting: ${desc} (${svc})${NC}"
    print_info "Binary: $binary"
    print_info "Port: $port"

    if [[ "$svc" == "web" ]]; then
        # 准备临时静态目录，解决 CSS/favicon 软链接问题
        local temp_static_dir="${PROJECT_ROOT}/data/run/web-static"
        rm -rf "$temp_static_dir"
        mkdir -p "$temp_static_dir"
        cp -L "${PROJECT_ROOT}/apps/web/index.html" "$temp_static_dir/"
        cp -rL "${PROJECT_ROOT}/apps/web/pkg" "$temp_static_dir/"
        cp -rL "${PROJECT_ROOT}/apps/web/style" "$temp_static_dir/"
        cp -L "${PROJECT_ROOT}/apps/web/style/main.css" "$temp_static_dir/style.css"
        cp -L "${PROJECT_ROOT}/apps/web/style/components.css" "$temp_static_dir/components.css"
        if [[ -f "${PROJECT_ROOT}/apps/web/public/favicon.svg" ]]; then
            cp -L "${PROJECT_ROOT}/apps/web/public/favicon.svg" "$temp_static_dir/favicon.svg"
        fi
        print_info "Static path: $temp_static_dir"
        print_info "Gateway URL: http://localhost:8000"
        nohup "$binary" --static-path "$temp_static_dir" --gateway-url http://localhost:8000 > "${PID_DIR}/${svc}.log" 2>&1 &
    else
        nohup "$binary" > "${PID_DIR}/${svc}.log" 2>&1 &
    fi
    local pid=$!
    echo $pid > "$pid_file"

    sleep 1
    if kill -0 "$pid" 2>/dev/null; then
        print_success "${svc} started (PID: $pid)"
    else
        print_error "${svc} failed to start. Check ${PID_DIR}/${svc}.log"
        rm -f "$pid_file"
        return 1
    fi
}

stop_service() {
    local svc="$1"
    local pid_file=$(get_pid_file "$svc")

    if ! is_running "$svc"; then
        print_warn "${svc} is not running"
        rm -f "$pid_file"
        return 0
    fi

    local pid=$(cat "$pid_file")
    echo -e "${CYAN}Stopping ${svc} (PID: $pid)...${NC}"

    if kill "$pid" 2>/dev/null; then
        local count=0
        while kill -0 "$pid" 2>/dev/null && [[ $count -lt 10 ]]; do
            sleep 0.5
            count=$((count + 1))
        done
        if kill -0 "$pid" 2>/dev/null; then
            kill -9 "$pid" 2>/dev/null || true
            print_warn "${svc} force stopped"
        else
            print_success "${svc} stopped"
        fi
    fi
    rm -f "$pid_file"
}

restart_service() {
    local svc="$1"
    stop_service "$svc"
    sleep 1
    start_service "$svc"
}

build_and_start() {
    local svc="$1"
    build_service "$svc" && start_service "$svc"
}

pack_release() {
    local target="${1:-all}"

    echo -e "${CYAN}----------------------------------------${NC}"
    echo -e "${CYAN}Packing release for target: ${target}${NC}"
    echo -e "${CYAN}----------------------------------------${NC}"

    local out_dir="${PROJECT_ROOT}/dist/beebotos"
    local archive="${PROJECT_ROOT}/dist/beebotos-$(uname -m)-unknown-linux-gnu.tar.gz"

    rm -rf "${out_dir}"
    mkdir -p "${out_dir}/pkg"

    # Copy binaries and assets
    if [[ "$target" == "all" || "$target" == "gateway" ]]; then
        cp "${PROJECT_ROOT}/target/release/beebotos-gateway" "${out_dir}/"
        cp -r "${PROJECT_ROOT}/migrations_sqlite" "${out_dir}/"
    fi
    if [[ "$target" == "all" || "$target" == "web" ]]; then
        if [[ ! -d "${PROJECT_ROOT}/apps/web/pkg" ]]; then
            print_error "WASM package directory not found: ${PROJECT_ROOT}/apps/web/pkg"
            print_info "Please build web service first: ./scripts/beebotos-dev.sh build web"
            return 1
        fi
        cp "${PROJECT_ROOT}/target/release/web-server" "${out_dir}/"
        cp -r "${PROJECT_ROOT}/apps/web/pkg/." "${out_dir}/pkg/"

        # 复制 web 入口页面和静态资源
        cp "${PROJECT_ROOT}/apps/web/index.html" "${out_dir}/"
        cp -rL "${PROJECT_ROOT}/apps/web/style" "${out_dir}/"
        cp -r "${PROJECT_ROOT}/apps/web/public" "${out_dir}/"

        # 复制根目录下的软链接文件（style.css -> style/main.css 等）
        for link_file in style.css components.css favicon.svg; do
            if [[ -L "${PROJECT_ROOT}/apps/web/${link_file}" ]]; then
                cp -L "${PROJECT_ROOT}/apps/web/${link_file}" "${out_dir}/${link_file}"
            fi
        done
    fi
    if [[ "$target" == "all" || "$target" == "beehub" ]]; then
        if [[ -f "${PROJECT_ROOT}/target/release/beehub" ]]; then
            cp "${PROJECT_ROOT}/target/release/beehub" "${out_dir}/"
        else
            print_warn "beehub binary not found, skipping"
        fi
    fi

    # Copy configs if they exist
    if [[ -d "${PROJECT_ROOT}/config" ]]; then
        cp -r "${PROJECT_ROOT}/config" "${out_dir}/"
    fi

    # Copy runner script
    cp "${PROJECT_ROOT}/scripts/beebotos-run.sh" "${out_dir}/"
    chmod +x "${out_dir}/beebotos-run.sh"

    # Create archive
    tar czvf "${archive}" -C "${PROJECT_ROOT}/dist" beebotos

    print_success "Release packed: ${archive}"
    echo "Contents:"
    ls -lah "${out_dir}"
}

show_status() {
    echo -e "${CYAN}Service Status${NC}"
    echo -e "${CYAN}----------------------------------------${NC}"
    printf "%-12s %-10s %-8s %s\n" "Service" "Status" "PID" "Port"
    echo "----------------------------------------"
    for entry in "${SERVICES[@]}"; do
        IFS='|' read -r name _ binary port desc <<< "$entry"
        if [[ -z "$binary" ]]; then
            printf "%-12s %-10s %-8s %s\n" "$name" "N/A" "-" "install-only"
            continue
        fi
        local pid_file=$(get_pid_file "$name")
        if is_running "$name"; then
            local pid=$(cat "$pid_file")
            printf "%-12s ${GREEN}%-10s${NC} %-8s %s\n" "$name" "running" "$pid" "$port"
        else
            printf "%-12s ${RED}%-10s${NC} %-8s %s\n" "$name" "stopped" "-" "$port"
        fi
    done
}

show_menu() {
    clear
    print_header
    echo "  1) Build"
    echo "     1.1) Build Gateway"
    echo "     1.2) Build Web"
    echo "     1.3) Build CLI"
    echo "     1.4) Build BeeHub"
    echo "     1.5) Build All"
    echo ""
    echo "  2) Start"
    echo "     2.1) Start Gateway"
    echo "     2.2) Start Web"
    echo "     2.3) Start BeeHub"
    echo "     2.4) Start All"
    echo ""
    echo "  3) Stop"
    echo "     3.1) Stop Gateway"
    echo "     3.2) Stop Web"
    echo "     3.3) Stop BeeHub"
    echo "     3.4) Stop All"
    echo ""
    echo "  4) Restart"
    echo "     4.1) Restart Gateway"
    echo "     4.2) Restart Web"
    echo "     4.3) Restart BeeHub"
    echo "     4.4) Restart All"
    echo ""
    echo "  5) Build & Start"
    echo "     5.1) Build & Start Gateway"
    echo "     5.2) Build & Start Web"
    echo "     5.3) Build & Start BeeHub"
    echo "     5.4) Build & Start All"
    echo ""
    echo "  6) Status"
    echo "  7) Pack Release"
    echo "  0) Exit"
    echo ""
    echo -n "Select option: "
}

handle_menu() {
    while true; do
        show_menu
        read -r choice
        echo ""

        case "$choice" in
            1|1.1) build_service gateway ;;
            1.2)  build_service web ;;
            1.3)  build_service cli ;;
            1.4)  build_service beehub ;;
            1.5)
                for svc in gateway web cli beehub; do
                    build_service "$svc" || true
                done
                ;;
            2|2.1) start_service gateway ;;
            2.2)  start_service web ;;
            2.3)  start_service beehub ;;
            2.4)
                for svc in gateway web beehub; do
                    start_service "$svc" || true
                done
                ;;
            3|3.1) stop_service gateway ;;
            3.2)  stop_service web ;;
            3.3)  stop_service beehub ;;
            3.4)
                for svc in gateway web beehub; do
                    stop_service "$svc"
                done
                ;;
            4|4.1) restart_service gateway ;;
            4.2)  restart_service web ;;
            4.3)  restart_service beehub ;;
            4.4)
                for svc in gateway web beehub; do
                    restart_service "$svc" || true
                done
                ;;
            5|5.1) build_and_start gateway ;;
            5.2)  build_and_start web ;;
            5.3)  build_and_start beehub ;;
            5.4)
                for svc in gateway web beehub; do
                    build_and_start "$svc" || true
                done
                ;;
            6) show_status ;;
            7) pack_release all ;;
            0|q|quit|exit) echo "Goodbye!"; exit 0 ;;
            *) print_warn "Invalid option: $choice" ;;
        esac

        echo ""
        read -p "Press Enter to continue..."
    done
}

handle_cli() {
    local action="$1"
    local target="${2:-all}"

    if [[ -n "$target" ]] && ! is_valid_service "$target" && [[ "$target" != "all" ]]; then
        print_error "Unknown service: $target"
        print_info "Available: $(service_names) all"
        exit 1
    fi

    case "$action" in
        build)
            if [[ "$target" == "all" ]]; then
                for svc in gateway web cli beehub; do
                    build_service "$svc" || true
                done
            else
                build_service "$target"
            fi
            ;;
        start)
            if [[ "$target" == "all" ]]; then
                for svc in gateway web beehub; do
                    start_service "$svc" || true
                done
            else
                start_service "$target"
            fi
            ;;
        stop)
            if [[ "$target" == "all" ]]; then
                for svc in gateway web beehub; do
                    stop_service "$svc"
                done
            else
                stop_service "$target"
            fi
            ;;
        restart)
            if [[ "$target" == "all" ]]; then
                for svc in gateway web beehub; do
                    restart_service "$svc" || true
                done
            else
                restart_service "$target"
            fi
            ;;
        run)
            if [[ "$target" == "all" ]]; then
                for svc in gateway web beehub; do
                    build_and_start "$svc" || true
                done
            else
                build_and_start "$target"
            fi
            ;;
        pack)
            pack_release "$target"
            ;;
        status)
            show_status
            ;;
        *)
            print_error "Unknown action: $action"
            echo "Usage: $0 [menu|build|start|stop|restart|run|pack|status] [service|all]"
            echo ""
            echo "Actions:"
            echo "  build    - Compile a service"
            echo "  start    - Start a service"
            echo "  stop     - Stop a service"
            echo "  restart  - Restart a service"
            echo "  run      - Build and start a service"
            echo "  pack     - Package binaries and assets for deployment"
            echo "  status   - Show service status"
            echo "  menu     - Interactive menu (default)"
            echo ""
            echo "Services: $(service_names) all"
            exit 1
            ;;
    esac
}

main() {
    local action="${1:-menu}"
    if [[ "$action" == "menu" ]]; then
        handle_menu
    else
        handle_cli "$@"
    fi
}

main "$@"
