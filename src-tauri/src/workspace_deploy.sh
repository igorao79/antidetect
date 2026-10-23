#!/usr/bin/env bash
# Installs or updates an Owl server on a Debian or Ubuntu machine. Owl runs it
# over SSH as root (`workspace_deploy.rs`) after uploading the server's sources
# as a tar file. It prints `OWL_STEP <step>` as it goes and ends with
# `OWL_LINK <owner link>`, or `OWL_HAS_OWNER` when the server already has one.
#
#   workspace_deploy.sh <bundle.tar> <public url> <domain or ""> <server name>
set -euo pipefail
bundle="$1" public_url="$2" domain="$3" name="$4"
dir=/opt/owl-server
compose() { docker compose -f "$dir/donut-sync/docker-compose.yml" --project-name owl "$@"; }

# shellcheck disable=SC1091
. /etc/os-release
case " ${ID:-} ${ID_LIKE:-} " in
  *" debian "* | *" ubuntu "*) ;;
  *)
    echo "OWL_ERROR unsupported_os ${PRETTY_NAME:-${ID:-unknown}}"
    exit 3
    ;;
esac

echo "OWL_STEP docker"
export DEBIAN_FRONTEND=noninteractive
if ! command -v curl > /dev/null; then
  apt-get update -q
  apt-get install -y -q curl ca-certificates
fi
if ! command -v docker > /dev/null || ! docker compose version > /dev/null 2>&1; then
  # Docker's own packages first; a release too new for them gets the
  # distribution's.
  if ! curl -fsSL https://get.docker.com | sh; then
    apt-get update -q
    apt-get install -y -q docker.io
    apt-get install -y -q docker-compose-v2 || apt-get install -y -q docker-compose-plugin \
      || apt-get install -y -q docker-compose
  fi
fi
if ! docker compose version > /dev/null 2>&1; then
  echo "OWL_ERROR no_compose"
  exit 5
fi
systemctl enable --now docker > /dev/null 2>&1 || true

echo "OWL_STEP server"
mkdir -p "$dir"
tar -xf "$bundle" -C "$dir"
rm -f "$bundle"
# The settings Owl manages are rewritten; anything else in .env is kept.
env_file="$dir/donut-sync/.env"
kept=$(grep -vE '^(PUBLIC_URL|WORKSPACE_NAME|DOMAIN|OWL_PORT)=' "$env_file" 2> /dev/null || true)
{
  [ -n "$kept" ] && printf '%s\n' "$kept"
  echo "PUBLIC_URL=$public_url"
  echo "WORKSPACE_NAME=$name"
  if [ -n "$domain" ]; then
    echo "DOMAIN=$domain"
    # Behind Caddy the server answers only on this machine.
    echo "OWL_PORT=127.0.0.1:12342"
  fi
} > "$env_file.new"
mv "$env_file.new" "$env_file"
chmod 600 "$env_file"

if command -v ufw > /dev/null && ufw status 2> /dev/null | grep -q "Status: active"; then
  if [ -n "$domain" ]; then
    ufw allow 80/tcp > /dev/null
    ufw allow 443/tcp > /dev/null
  else
    ufw allow 12342/tcp > /dev/null
  fi
fi

profile=()
[ -n "$domain" ] && profile=(--profile https)
compose "${profile[@]}" up -d --build --remove-orphans

healthy=""
for _ in $(seq 1 90); do
  if compose exec -T owl-server wget -qO- http://127.0.0.1:12342/health > /dev/null 2>&1; then
    healthy=1
    break
  fi
  sleep 2
done
if [ -z "$healthy" ]; then
  compose logs --tail 20 owl-server || true
  echo "OWL_ERROR server_unhealthy"
  exit 4
fi

echo "OWL_STEP owner"
out=$(compose exec -T owl-server node dist/cli owner-link 2>&1 || true)
link=$(printf '%s\n' "$out" | grep -o 'owl://join[^[:space:]]*' | head -n 1 || true)
if [ -n "$link" ]; then
  echo "OWL_LINK $link"
else
  echo "OWL_HAS_OWNER"
fi
