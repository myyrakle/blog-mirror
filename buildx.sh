#!/usr/bin/env bash
#
# Builds and pushes the release image.
#
#   ./buildx.sh            # version from Cargo.toml
#   ./buildx.sh 0.2.2      # explicit version
#   PLATFORMS=linux/amd64,linux/arm64 ./buildx.sh
#
# The build context is exported to a temporary directory first. buildx derives
# its gRPC session key from the context path, and a non-ASCII path (a git
# worktree named 웹UI, for instance) makes it fail with:
#
#   header key "x-docker-expose-session-sharedkey" contains value with
#   non-printable ASCII characters
#
# Exporting with `git archive` also keeps uncommitted files out of the image.
set -euo pipefail

usage() {
    sed -n '3,9p' "$0" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

case "${1:-}" in
    -h | --help) usage 0 ;;
esac

VERSION="${1:-$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)}"

# Guard against a stray flag or typo becoming an image tag and being pushed.
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.-]+)?$ ]]; then
    echo "error: '$VERSION' is not a version (expected e.g. 0.2.2)" >&2
    usage 1 >&2
fi

IMAGE="myyrakle/blog-mirror:v${VERSION}"

# arm64 needs QEMU registered (`docker run --privileged --rm tonistiigi/binfmt
# --install arm64`); the deploy target is a single x86_64 node, so default to
# amd64 only rather than failing on a platform nobody uses.
PLATFORMS="${PLATFORMS:-linux/amd64}"

# No sudo: being in the `docker` group is enough, and running as root would use
# root's builder with a separate cache.
BUILDER="${BUILDER:-default}"

BUILD_DIR="$(mktemp -d)"
trap 'rm -rf "$BUILD_DIR"' EXIT
git archive --format=tar HEAD | tar -x -C "$BUILD_DIR"

echo "Building ${IMAGE} for ${PLATFORMS} from $(git rev-parse --short HEAD)"
docker buildx build \
  --builder "$BUILDER" \
  --platform "$PLATFORMS" \
  -t "$IMAGE" \
  --push \
  "$BUILD_DIR"
