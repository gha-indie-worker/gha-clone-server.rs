#!/usr/bin/env bash
set -Eeuo pipefail
umask 077

readonly script_name="${0##*/}"
scope=''
target=''
webhook_url=''
secret_file=''
temp_dir=''

cleanup() {
  if [[ -n "$temp_dir" && -d "$temp_dir" ]]; then
    rm -rf -- "$temp_dir"
  fi
}
trap cleanup EXIT HUP INT TERM

usage() {
  cat >&2 <<'USAGE'
usage: register-github-webhook.sh \
  (--repo OWNER/REPO | --org ORGANIZATION) \
  --url https://host/webhooks/github \
  --secret-file PATH

Authentication is supplied through GH_TOKEN or an existing `gh auth login`.
The HMAC secret must be a private, non-symlink regular file containing one
visible-ASCII line of 32 to 4096 bytes. It is never accepted from an environment
variable, printed, or placed in a process argument.
USAGE
}

while (($#)); do
  case "$1" in
    --repo|--org)
      [[ $# -ge 2 ]] || { echo "$script_name: $1 requires a value" >&2; exit 64; }
      [[ -z "$scope" ]] || { echo "$script_name: choose exactly one of --repo or --org" >&2; exit 64; }
      scope="${1#--}"
      target="$2"
      shift 2
      ;;
    --url)
      [[ $# -ge 2 ]] || { echo "$script_name: --url requires a value" >&2; exit 64; }
      webhook_url="$2"
      shift 2
      ;;
    --secret-file)
      [[ $# -ge 2 ]] || { echo "$script_name: --secret-file requires a value" >&2; exit 64; }
      secret_file="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "$script_name: unknown argument: $1" >&2
      usage
      exit 64
      ;;
  esac
done

command -v gh >/dev/null 2>&1 || { echo "$script_name: gh is required" >&2; exit 69; }
command -v jq >/dev/null 2>&1 || { echo "$script_name: jq is required" >&2; exit 69; }
command -v python3 >/dev/null 2>&1 || { echo "$script_name: python3 is required" >&2; exit 69; }

[[ -n "$scope" && -n "$target" && -n "$webhook_url" && -n "$secret_file" ]] || {
  usage
  exit 64
}
if [[ "$scope" == repo ]]; then
  [[ "$target" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || {
    echo "$script_name: --repo requires exact OWNER/REPO" >&2
    exit 64
  }
  endpoint="/repos/${target}/hooks"
else
  [[ "$target" =~ ^[A-Za-z0-9_.-]+$ ]] || {
    echo "$script_name: --org requires an exact organization login" >&2
    exit 64
  }
  endpoint="/orgs/${target}/hooks"
fi
[[ -f "$secret_file" && -r "$secret_file" && ! -L "$secret_file" ]] || {
  echo "$script_name: --secret-file must name a readable, non-symlink regular file" >&2
  exit 66
}

python3 - "$webhook_url" <<'PY'
from __future__ import annotations

import sys
from urllib.parse import urlsplit

parsed = urlsplit(sys.argv[1])
if (
    parsed.scheme != "https"
    or not parsed.hostname
    or parsed.username is not None
    or parsed.password is not None
    or parsed.query
    or parsed.fragment
    or parsed.path not in {"/webhooks/github", "/gha-webhooks/github"}
):
    raise SystemExit(
        "webhook URL must be credential-free HTTPS with exact /webhooks/github or /gha-webhooks/github path"
    )
PY

temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/${script_name}.XXXXXX")"
normalized_secret_file="$temp_dir/webhook-secret"
payload_file="$temp_dir/hook.json"

python3 - "$secret_file" "$normalized_secret_file" <<'PY'
from __future__ import annotations

import os
import sys
from pathlib import Path

value = Path(sys.argv[1]).read_bytes()
if value.endswith(b"\r\n"):
    value = value[:-2]
elif value.endswith(b"\n"):
    value = value[:-1]
if not 32 <= len(value) <= 4096:
    raise SystemExit("webhook secret must contain 32 to 4096 bytes after terminal-newline normalization")
if any(byte < 0x21 or byte > 0x7E for byte in value):
    raise SystemExit("webhook secret must be a single visible-ASCII line")
destination = Path(sys.argv[2])
destination.write_bytes(value)
os.chmod(destination, 0o600)
PY

existing_ids="$(
  gh api "${endpoint}?per_page=100" --paginate \
    | jq -r --arg url "$webhook_url" '.[] | select(.config.url == $url) | .id'
)"
match_count="$(printf '%s\n' "$existing_ids" | awk 'NF { count += 1 } END { print count + 0 }')"
if ((match_count > 1)); then
  echo "$script_name: multiple hooks already use the exact callback URL; refusing an ambiguous update" >&2
  exit 65
fi
existing_id="$(printf '%s\n' "$existing_ids" | awk 'NF { print; exit }')"

jq -cn \
  --arg url "$webhook_url" \
  --rawfile secret "$normalized_secret_file" \
  '{
    name: "web",
    active: true,
    events: ["workflow_run"],
    config: {
      url: $url,
      content_type: "json",
      secret: $secret,
      insecure_ssl: "0"
    }
  }' >"$payload_file"

if [[ -n "$existing_id" ]]; then
  [[ "$existing_id" =~ ^[0-9]+$ ]] || { echo "$script_name: GitHub returned a non-numeric hook id" >&2; exit 1; }
  result="$(gh api --method PATCH "${endpoint}/${existing_id}" --input "$payload_file")"
  action='updated'
else
  result="$(gh api --method POST "$endpoint" --input "$payload_file")"
  action='created'
fi

jq -e --arg url "$webhook_url" '
  (.id | type == "number") and
  .active == true and
  .config.url == $url and
  .config.content_type == "json" and
  .config.insecure_ssl == "0" and
  .events == ["workflow_run"]
' <<<"$result" >/dev/null || {
  echo "$script_name: GitHub returned a webhook that does not match the requested contract" >&2
  exit 1
}

hook_id="$(jq -r '.id' <<<"$result")"
printf '%s workflow_run webhook id=%s scope=%s target=%s url=%s\n' \
  "$action" "$hook_id" "$scope" "$target" "$webhook_url"
