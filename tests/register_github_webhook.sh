#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$root/scripts/register-github-webhook.sh"
tmp="$(mktemp -d)"
trap 'rm -rf -- "$tmp"' EXIT HUP INT TERM
mkdir -p "$tmp/bin"

cat >"$tmp/bin/gh" <<'MOCK'
#!/usr/bin/env bash
set -Eeuo pipefail
printf '%s\n' "$*" >>"${GH_MOCK_ARGS}"
if [[ "$*" == *"?per_page=100"* ]]; then
  if [[ "${GH_MOCK_DUPLICATE:-0}" == 1 ]]; then
    printf '[{"id":11,"config":{"url":"%s"}},{"id":12,"config":{"url":"%s"}}]\n' \
      "$GH_MOCK_URL" "$GH_MOCK_URL"
  else
    printf '[]\n'
  fi
  exit 0
fi
input=''
while (($#)); do
  if [[ "$1" == --input ]]; then
    input="$2"
    break
  fi
  shift
done
[[ -n "$input" ]]
cp "$input" "$GH_MOCK_PAYLOAD"
jq -e --arg url "$GH_MOCK_URL" --arg secret "$GH_MOCK_SECRET" '
  .events == ["workflow_run"] and
  .active == true and
  .config.url == $url and
  .config.content_type == "json" and
  .config.insecure_ssl == "0" and
  .config.secret == $secret
' "$input" >/dev/null
printf '{"id":77,"active":true,"events":["workflow_run"],"config":{"url":"%s","content_type":"json","insecure_ssl":"0"}}\n' "$GH_MOCK_URL"
MOCK
chmod +x "$tmp/bin/gh"

export PATH="$tmp/bin:$PATH"
export GH_MOCK_ARGS="$tmp/args"
export GH_MOCK_PAYLOAD="$tmp/payload.json"
export GH_MOCK_URL='https://ci.example.test/webhooks/github'
export GH_MOCK_SECRET='unit-test-webhook-secret-xxxxxxxx'
printf '%s\n' "$GH_MOCK_SECRET" >"$tmp/secret"
chmod 600 "$tmp/secret"

output="$($script --repo example/repository --url "$GH_MOCK_URL" --secret-file "$tmp/secret")"
[[ "$output" == *'created workflow_run webhook id=77'* ]]
! grep -Fq "$GH_MOCK_SECRET" "$GH_MOCK_ARGS"

export GH_MOCK_DUPLICATE=1
if "$script" --repo example/repository --url "$GH_MOCK_URL" --secret-file "$tmp/secret" >"$tmp/out" 2>"$tmp/err"; then
  echo 'duplicate callback URLs unexpectedly succeeded' >&2
  exit 1
fi
grep -Fq 'multiple hooks already use the exact callback URL' "$tmp/err"
unset GH_MOCK_DUPLICATE

printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n' >"$tmp/short"
chmod 600 "$tmp/short"
if "$script" --repo example/repository --url "$GH_MOCK_URL" --secret-file "$tmp/short" >"$tmp/out" 2>"$tmp/err"; then
  echo '31-byte secret plus newline unexpectedly succeeded' >&2
  exit 1
fi
grep -Fq '32 to 4096 bytes' "$tmp/err"

cp "$tmp/secret" "$tmp/public-secret"
chmod 644 "$tmp/public-secret"
if "$script" --repo example/repository --url "$GH_MOCK_URL" --secret-file "$tmp/public-secret" >"$tmp/out" 2>"$tmp/err"; then
  echo 'group/world-readable secret unexpectedly succeeded' >&2
  exit 1
fi
grep -Fq 'must not be group/world accessible' "$tmp/err"

ln -s "$tmp/secret" "$tmp/secret-link"
if "$script" --repo example/repository --url "$GH_MOCK_URL" --secret-file "$tmp/secret-link" >"$tmp/out" 2>"$tmp/err"; then
  echo 'symlink secret unexpectedly succeeded' >&2
  exit 1
fi
grep -Fq 'non-symlink regular file' "$tmp/err"

printf 'register webhook contract: ok\n'
