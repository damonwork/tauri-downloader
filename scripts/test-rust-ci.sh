#!/usr/bin/env bash

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE_NAME="fluxor-rust-ci:run-$$"
CONTAINER_NAME="fluxor-rust-ci-$$"

cleanup() {
    docker rm --force "$CONTAINER_NAME" >/dev/null 2>&1 || true
    docker image rm "$IMAGE_NAME" >/dev/null 2>&1 || true
}

trap cleanup EXIT INT TERM

docker build --tag "$IMAGE_NAME" --file "$PROJECT_ROOT/Dockerfile.rust-ci" "$PROJECT_ROOT"
docker create --name "$CONTAINER_NAME" "$IMAGE_NAME" tail -f /dev/null >/dev/null
docker start "$CONTAINER_NAME" >/dev/null

# Keep the host independent from the container: source enters with docker cp,
# and only rustfmt changes are copied back before the checks continue.
docker exec "$CONTAINER_NAME" rm -rf /workspace/src-tauri
docker exec "$CONTAINER_NAME" mkdir -p /workspace/src-tauri
for path in Cargo.toml Cargo.lock build.rs tauri.conf.json capabilities icons src; do
    docker cp "$PROJECT_ROOT/src-tauri/$path" "$CONTAINER_NAME:/workspace/src-tauri/$path"
done

docker exec --workdir /workspace/src-tauri "$CONTAINER_NAME" cargo fmt --all
docker cp "$CONTAINER_NAME:/workspace/src-tauri/build.rs" "$PROJECT_ROOT/src-tauri/build.rs"
docker cp "$CONTAINER_NAME:/workspace/src-tauri/src/." "$PROJECT_ROOT/src-tauri/src/"

docker exec --workdir /workspace/src-tauri "$CONTAINER_NAME" \
    cargo clippy --locked --all-targets --all-features -- -D warnings
docker exec --workdir /workspace/src-tauri "$CONTAINER_NAME" cargo test --locked --lib
docker exec --workdir /workspace/src-tauri "$CONTAINER_NAME" cargo check --locked
