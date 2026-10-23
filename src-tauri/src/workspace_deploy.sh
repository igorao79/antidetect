#!/usr/bin/env bash
# Installs or updates an Owl server on a Debian or Ubuntu machine. Owl runs it
# over SSH as root (`workspace_deploy.rs`) after uploading the server's sources
# as a tar file. It prints `OWL_STEP <step>` as it goes and ends with
# `OWL_LINK <owner link>`, or `OWL_HAS_OWNER` when the server already has one.
#
# HTTPS comes from Caddy in a container on a machine whose ports 80 and 443
# are free, and from the nginx already there when nginx holds them: Owl adds
# a site of its own for the domain and certbot gets its certificate, leaving
# the machine's other sites alone. What the script installed is listed in
# /opt/owl-server/installed, so it can be taken off again.
#
#   workspace_deploy.sh <bundle.tar> <public url> <domain or ""> <server name>
set -euo pipefail
bundle="$1" public_url="$2" domain="$3" name="$4"
dir=/opt/owl-server
record="$dir/installed"
compose() { docker compose -f "$dir/donut-sync/docker-compose.yml" --project-name owl "$@"; }
remember() { grep -qxF "$1" "$record" 2> /dev/null || echo "$1" >> "$record"; }

# shellcheck disable=SC1091
. /etc/os-release
case " ${ID:-} ${ID_LIKE:-} " in
  *" debian "* | *" ubuntu "*) ;;
  *)
    echo "OWL_ERROR unsupported_os ${PRETTY_NAME:-${ID:-unknown}}"
    exit 3
    ;;
esac

# Who holds ports 80 and 443 decides how HTTPS is served, and is checked
# before anything is installed.
web=caddy
if [ -n "$domain" ]; then
  holders=$(ss -ltnpH '( sport = :80 or sport = :443 )' 2> /dev/null \
    | grep -o 'users:(("[^"]*"' | cut -d'"' -f2 | sort -u | tr '\n' ' ' || true)
  for holder in $holders; do
    case "$holder" in
      nginx) web=nginx ;;
      docker-proxy) ;; # Caddy from an earlier install
      *)
        echo "OWL_ERROR ports_busy $holder"
        exit 6
        ;;
    esac
  done
fi
mkdir -p "$dir"

echo "OWL_STEP docker"
export DEBIAN_FRONTEND=noninteractive
if ! command -v curl > /dev/null; then
  apt-get update -q
  apt-get install -y -q curl ca-certificates
fi
if ! command -v docker > /dev/null || ! docker compose version > /dev/null 2>&1; then
  remember docker
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
    # Behind Caddy or nginx the server answers only on this machine.
    echo "OWL_PORT=127.0.0.1:12342"
  fi
} > "$env_file.new"
mv "$env_file.new" "$env_file"
chmod 600 "$env_file"

# nginx already answers on 80 and 443, so the firewall lets them through.
if [ "$web" = caddy ] && command -v ufw > /dev/null \
  && ufw status 2> /dev/null | grep -q "Status: active"; then
  if [ -n "$domain" ]; then
    ufw allow 80/tcp > /dev/null
    ufw allow 443/tcp > /dev/null
  else
    ufw allow 12342/tcp > /dev/null
  fi
fi

profile=()
[ -n "$domain" ] && [ "$web" = caddy ] && profile=(--profile https)
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

if [ "$web" = nginx ]; then
  echo "OWL_STEP https"
  if [ -d /etc/nginx/sites-available ] && [ -d /etc/nginx/sites-enabled ]; then
    site=/etc/nginx/sites-available/owl-server.conf
    enabled=/etc/nginx/sites-enabled/owl-server.conf
  else
    site=/etc/nginx/conf.d/owl-server.conf
    enabled=""
  fi
  # Written once: certbot adds its HTTPS lines to it, and an update keeps them.
  if [ ! -f "$site" ]; then
    cat > "$site" << EOF
# Owl server, added by Owl (workspace_deploy.sh). Remove it with the server.
server {
    listen 80;
    listen [::]:80;
    server_name $domain;

    # Profile files of any size; the event stream stays open for hours.
    client_max_body_size 0;
    location / {
        proxy_pass http://127.0.0.1:12342;
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
        proxy_buffering off;
        proxy_request_buffering off;
        proxy_read_timeout 1h;
        proxy_send_timeout 1h;
    }
}
EOF
  fi
  [ -n "$enabled" ] && ln -sf "$site" "$enabled"
  remember "nginx-site $site"
  [ -n "$enabled" ] && remember "nginx-site $enabled"
  # Never leave nginx with a configuration it refuses: the other sites on
  # this machine keep running whatever happens here.
  if ! nginx -t > /dev/null 2>&1; then
    rm -f "$site" ${enabled:+"$enabled"}
    nginx -t 2>&1 | tail -n 3 || true
    echo "OWL_ERROR nginx_config"
    exit 7
  fi
  systemctl reload nginx
  if ! command -v certbot > /dev/null; then
    remember certbot
    apt-get update -q
    apt-get install -y -q certbot python3-certbot-nginx
  elif ! certbot plugins 2> /dev/null | grep -q nginx; then
    remember certbot-nginx
    apt-get install -y -q python3-certbot-nginx
  fi
  if ! certbot --nginx -d "$domain" --non-interactive --agree-tos \
    --register-unsafely-without-email --redirect --keep-until-expiring; then
    echo "OWL_ERROR certificate"
    exit 8
  fi
  remember "certificate $domain"
fi

echo "OWL_STEP owner"
out=$(compose exec -T owl-server node dist/cli owner-link 2>&1 || true)
link=$(printf '%s\n' "$out" | grep -o 'owl://join[^[:space:]]*' | head -n 1 || true)
if [ -n "$link" ]; then
  echo "OWL_LINK $link"
else
  echo "OWL_HAS_OWNER"
fi
