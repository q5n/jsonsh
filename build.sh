#!/usr/bin/env bash

set -euo pipefail

show_usage() {
    cat <<'EOF'
Usage: bash build.sh [options]

Options:
  --skip-tests       Build without running the test suite
  --version VERSION  Inject an explicit version (default: Git description)
  -h, --help         Show this help
EOF
}

skip_tests=false
build_version=""

while (($# > 0)); do
    case "$1" in
        --skip-tests)
            skip_tests=true
            shift
            ;;
        --version)
            if (($# < 2)); then
                echo "build.sh: --version requires a value" >&2
                exit 2
            fi
            build_version=$2
            shift 2
            ;;
        --version=*)
            build_version=${1#*=}
            shift
            ;;
        -h|--help)
            show_usage
            exit 0
            ;;
        *)
            echo "build.sh: unknown option: $1" >&2
            show_usage >&2
            exit 2
            ;;
    esac
done

if ! command -v go >/dev/null 2>&1; then
    echo "build.sh: Go was not found. Install Go and add it to PATH." >&2
    exit 1
fi

project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
cd "$project_root"

if [[ -z "$build_version" ]]; then
    if command -v git >/dev/null 2>&1; then
        build_version=$(git describe --tags --always --dirty 2>/dev/null || true)
    fi
    if [[ -z "$build_version" ]]; then
        build_version=dev
    fi
fi

target_os=$(go env GOOS)
output_name=jsonsh
if [[ "$target_os" == windows ]]; then
    output_name=jsonsh.exe
fi

output_dir="$project_root/dist"
output_file="$output_dir/$output_name"

if [[ "$skip_tests" == true ]]; then
    echo "[1/2] Tests skipped."
else
    echo "[1/2] Running tests..."
    go test ./...
fi

echo "[2/2] Building jsonsh for $target_os..."
mkdir -p "$output_dir"
go build -trimpath -ldflags "-X main.version=$build_version" -o "$output_file" ./cmd/jsonsh

echo
echo "Build succeeded: $output_file"
echo "Version: $build_version"
sleep 2