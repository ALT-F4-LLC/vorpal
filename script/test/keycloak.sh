#!/usr/bin/env bash
set -eo pipefail

if [ -z "${ARCHIVE_CLIENT_SECRET}" ]; then
    echo "missing ARCHIVE_CLIENT_SECRET env var"
    exit 1
fi

if [ -z "${ARTIFACT_CLIENT_SECRET}" ]; then
    echo "missing ARTIFACT_CLIENT_SECRET env var"
    exit 1
fi

if [ -z "${WORKER_CLIENT_SECRET}" ]; then
    echo "missing WORKER_CLIENT_SECRET env var"
    exit 1
fi

# =========================
# Config (edit these)
# =========================
: "${KC_BASE:=http://localhost:8080}"
: "${KC_REALM:=vorpal}"

# Clients / audiences
: "${ARCHIVE_CLIENT_ID:=archive}"
: "${ARTIFACT_CLIENT_ID:=artifact}"
: "${CLI_CLIENT_ID:=cli}"
: "${WORKER_CLIENT_ID:=worker}"

# Scopes (client scopes attached as optional to cli/agent/worker)
: "${SCOPE_AUD_ARCHIVE:=archive}"
: "${SCOPE_AUD_ARTIFACT:=artifact}"
: "${SCOPE_AUD_WORKER:=worker}"

#############################################
#           Derived Keycloak URLs           #
#############################################
TOKEN_ENDPOINT="$KC_BASE/realms/$KC_REALM/protocol/openid-connect/token"
DEVICE_ENDPOINT="$KC_BASE/realms/$KC_REALM/protocol/openid-connect/auth/device"
USERINFO_ENDPOINT="$KC_BASE/realms/$KC_REALM/protocol/openid-connect/userinfo"
INTROSPECT_ENDPOINT="$KC_BASE/realms/$KC_REALM/protocol/openid-connect/token/introspect"

#############################################
#                  Helpers                  #
#############################################
need() { command -v "$1" >/dev/null 2>&1 || { echo "Missing $1; install it."; exit 1; }; }
need curl; need jq; need date; need tr; need awk

b64dec() {
  if base64 --help 2>&1 | grep -q -- '--decode'; then base64 --decode; else base64 -D; fi
}

b64url_decode() {
  local in="$1"
  # URL-safe -> standard
  in="${in//-/+}"; in="${in//_//}"
  # pad
  case $((${#in} % 4)) in
    2) in="$in==";;
    3) in="$in=";;
  esac
  printf '%s' "$in" | b64dec 2>/dev/null || true
}

jwt_payload_json() {
  local tok="$1"
  IFS='.' read -r _ payload _ <<< "$tok"
  b64url_decode "$payload"
}

ts_to_iso() {
  local ts="$1"
  date -u -r "$ts" '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || date -u -d "@$ts" '+%Y-%m-%dT%H:%M:%SZ'
}

show_token_summary() {
  local label="$1" tok="$2"
  echo; echo "----- $label -----"
  local payload
  payload="$(jwt_payload_json "$tok")"
  if [ -z "$payload" ]; then echo "(unable to decode)"; return; fi
  echo "$payload" | jq '{
    iss, azp, sub,
    aud,
    exp, exp_iso: (try (.exp | tonumber) | . // empty),
    iat, iat_iso: (try (.iat | tonumber) | . // empty),
    resource_access
  }' | jq --argjson now "$(date +%s)" '
    .exp_iso = ( if .exp then "'"$(ts_to_iso "$(echo "$payload" | jq -r '.exp')")"'" else null end ) |
    .iat_iso = ( if .iat then "'"$(ts_to_iso "$(echo "$payload" | jq -r '.iat')")"'" else null end )'
}

say() { printf '\n==> %s\n' "$*"; }

#############################################
#      1) Start Device Authorization        #
#############################################
say "Starting Device Authorization (scope: openid ${SCOPE_AUD_ARCHIVE} ${SCOPE_AUD_ARTIFACT} ${SCOPE_AUD_WORKER})"

DEVICE_RESPONSE="$(
  curl -sS -X POST "$DEVICE_ENDPOINT" \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    --data "client_id=$CLI_CLIENT_ID" \
    --data-urlencode "scope=openid ${SCOPE_AUD_ARCHIVE} ${SCOPE_AUD_ARTIFACT} ${SCOPE_AUD_WORKER}" \
)"
DEVICE_CODE="$(jq -r '.device_code' <<<"$DEVICE_RESPONSE")"
USER_CODE="$(jq -r '.user_code' <<<"$DEVICE_RESPONSE")"
VERIF_URI="$(jq -r '.verification_uri' <<<"$DEVICE_RESPONSE")"
VERIF_URI_FULL="$(jq -r '.verification_uri_complete' <<<"$DEVICE_RESPONSE")"
INTERVAL="$(jq -r '.interval // 5' <<<"$DEVICE_RESPONSE")"

[ "$DEVICE_CODE" = "null" -o -z "$DEVICE_CODE" ] && { echo "Device flow failed:"; echo "$DEVICE_RESPONSE"; exit 1; }

cat <<EOF

Open this URL and authenticate:
  $VERIF_URI_FULL

If needed, go to:
  $VERIF_URI
and enter code:
  $USER_CODE

Polling token endpoint every ${INTERVAL}s...
EOF

#############################################
#       2) Poll for USER access token       #
#############################################
USER_AT=""

while :; do
  sleep "$INTERVAL"

  RESP="$(curl -sS -X POST "$TOKEN_ENDPOINT" \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    --data 'grant_type=urn:ietf:params:oauth:grant-type:device_code' \
    --data-urlencode "client_id=$CLI_CLIENT_ID" \
    --data-urlencode "device_code=$DEVICE_CODE" \
    -w '\n%{http_code}'
  )"

  HTTP="${RESP##*$'\n'}"
  BODY="${RESP%$'\n'*}"

  if [ "$HTTP" = "200" ]; then
    USER_AT="$(jq -r '.access_token' <<<"$BODY")"

    [ "$USER_AT" = "null" -o -z "$USER_AT" ] && { echo "Missing access_token"; echo "$BODY"; exit 1; }

    break
  else
    ERR="$(jq -r '.error // empty' <<<"$BODY")"

    case "$ERR" in
      authorization_pending) printf '.';;
      slow_down) INTERVAL=$((INTERVAL+2)); printf '\n(slow_down) interval=%ss\n' "$INTERVAL";;
      access_denied) echo; echo "Access denied"; exit 1;;
      expired_token) echo; echo "Device code expired"; exit 1;;
      *) echo; echo "Unexpected error ($HTTP): $BODY"; exit 1;;
    esac
  fi
done
echo

say "User access token acquired."

show_token_summary "USER_AT" "$USER_AT"

say "Calling Keycloak UserInfo with USER_AT"

curl -sS -H "Authorization: Bearer $USER_AT" "$USERINFO_ENDPOINT" | jq .

#############################################
# 3) Token Exchange (worker -> artifact)    #
#############################################

say "Token Exchange as WORKER -> audience=$ARTIFACT_CLIENT_ID (scope=${SCOPE_AUD_ARTIFACT})"

WORKER_ARTIFACT_AT="$(
  curl -sS -u "$WORKER_CLIENT_ID:$WORKER_CLIENT_SECRET" -X POST "$TOKEN_ENDPOINT" \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    --data 'grant_type=urn:ietf:params:oauth:grant-type:token-exchange' \
    --data 'requested_token_type=urn:ietf:params:oauth:token-type:access_token' \
    --data 'subject_token_type=urn:ietf:params:oauth:token-type:access_token' \
    --data-urlencode "subject_token=$USER_AT" \
    --data-urlencode "audience=$ARTIFACT_CLIENT_ID" \
    --data-urlencode "scope=${SCOPE_AUD_ARTIFACT}" \
  | jq -r '.access_token'
)"

[ -z "$WORKER_ARTIFACT_AT" -o "$WORKER_ARTIFACT_AT" = "null" ] && { echo "Token Exchange (artifact) failed."; exit 1; }

show_token_summary "WORKER_ARTIFACT_AT (exchanged)" "$WORKER_ARTIFACT_AT"

#############################################
# 4) Token Exchange (worker -> archive)    #
#############################################

say "Token Exchange as WORKER -> audience=$ARCHIVE_CLIENT_ID (scope=${SCOPE_AUD_ARCHIVE})"

WORKER_ARCHIVE_AT="$(
  curl -sS -u "$WORKER_CLIENT_ID:$WORKER_CLIENT_SECRET" -X POST "$TOKEN_ENDPOINT" \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    --data 'grant_type=urn:ietf:params:oauth:grant-type:token-exchange' \
    --data 'requested_token_type=urn:ietf:params:oauth:token-type:access_token' \
    --data 'subject_token_type=urn:ietf:params:oauth:token-type:access_token' \
    --data-urlencode "subject_token=$USER_AT" \
    --data-urlencode "audience=$ARCHIVE_CLIENT_ID" \
    --data-urlencode "scope=${SCOPE_AUD_ARCHIVE}" \
  | jq -r '.access_token'
)"

[ -z "$WORKER_ARCHIVE_AT" -o "$WORKER_ARCHIVE_AT" = "null" ] && { echo "Token Exchange (archive) failed."; exit 1; }

show_token_summary "WORKER_ARCHIVE_AT (exchanged)" "$WORKER_ARCHIVE_AT"

#############################################
# 5) Optional: introspect exchanged tokens  #
#############################################

# Provide target service secrets if you want introspection (still Keycloak-only)
if [[ -n "${ARCHIVE_CLIENT_SECRET:-}" ]]; then
  say "Introspecting ARCHIVE_AT as $ARCHIVE_CLIENT_ID"

  curl -sS -u "$ARCHIVE_CLIENT_ID:$ARCHIVE_CLIENT_SECRET" \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    --data "token=$WORKER_ARCHIVE_AT" "$INTROSPECT_ENDPOINT" | jq .
fi

if [[ -n "${ARTIFACT_CLIENT_SECRET:-}" ]]; then
  say "Introspecting ARTIFACT_AT as $ARTIFACT_CLIENT_ID"

  curl -sS -u "$ARTIFACT_CLIENT_ID:$ARTIFACT_CLIENT_SECRET" \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    --data "token=$WORKER_ARTIFACT_AT" "$INTROSPECT_ENDPOINT" | jq .
fi

echo "All steps completed (Keycloak-only)."
