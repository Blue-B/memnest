#!/usr/bin/env bash
# Shared by setup and the lower-level installer, before either changes files.
resolve_linux_service_config() {
  local template unit key stored server_port line unit_paths dir dropin
  case "$MODE" in user|system) ;; *) echo "MODE must be user or system" >&2; return 1 ;; esac
  if [ "$MODE" = system ]; then
    SERVICE_DIR=/etc/systemd/system
    template="$ROOT/packaging/systemd/memnest.service"
  else
    SERVICE_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
    template="$ROOT/packaging/systemd/memnest-user.service"
  fi
  unit_paths="$(systemd-analyze "--$MODE" unit-paths)" || {
    echo "Cannot inspect systemd unit paths; install systemd-analyze or upgrade manually." >&2; return 1;
  }
  grep -Fxq "$SERVICE_DIR" <<< "$unit_paths" || {
    echo "Nonstandard systemd unit path; upgrade manually instead of creating an ignored service." >&2; return 1;
  }
  HOST="${MEMNEST_HOST:-127.0.0.1}"
  PORT="${MEMNEST_PORT:-3111}"
  for unit in "$SERVICE_DIR/memnest.service" "$SERVICE_DIR/memnest-watch.service"; do
    [ "$MODE" != system ] || [ "$unit" != "$SERVICE_DIR/memnest-watch.service" ] || continue
    while IFS= read -r dir; do
      [ ! -d "$dir" ] || { [ -r "$dir" ] && [ -x "$dir" ]; } || {
        echo "Cannot inspect systemd path: $dir" >&2; return 1;
      }
      if [ "$dir" != "$SERVICE_DIR" ] && { [ -e "$dir/$(basename "$unit")" ] || [ -L "$dir/$(basename "$unit")" ]; }; then
        echo "Alternative service fragment needs a manual upgrade: $dir/$(basename "$unit")" >&2; return 1
      fi
      for dropin in "$dir/$(basename "$unit").d" "$dir/memnest-.service.d" "$dir/service.d"; do
        [ ! -d "$dropin" ] || { [ -r "$dropin" ] && [ -x "$dropin" ]; } || {
          echo "Cannot inspect systemd drop-ins: $dropin" >&2; return 1;
        }
        if compgen -G "$dropin/*.conf" >/dev/null; then
          echo "Custom systemd drop-ins need a manual upgrade; no files changed: $dropin" >&2; return 1
        fi
      done
    done <<< "$unit_paths"
    [ ! -L "$unit" ] || { echo "Refusing symlink service: $unit" >&2; return 1; }
    [ -e "$unit" ] || continue
    [ -f "$unit" ] && [ -r "$unit" ] || { echo "Cannot safely read service: $unit" >&2; return 1; }
    if grep -Eq '^[[:space:]]*(EnvironmentFile|UnsetEnvironment|PassEnvironment)[[:space:]]*=|^[[:space:]]*Environment[[:space:]]*=[[:space:]]*$|\\$' "$unit" ||
       grep -Eq "^[[:space:]]*Environment[[:space:]]*=[[:space:]]*(\"\"|'')[[:space:]]*$" "$unit"; then
      echo "External, reset or continued environment needs a manual upgrade; no files changed: $unit" >&2
      return 1
    fi
    if [ "$unit" = "$SERVICE_DIR/memnest-watch.service" ]; then
      template="$ROOT/packaging/systemd/memnest-watch-user.service"
    fi
    # Do not guess the endpoint of a customized command or systemd environment syntax.
    if [ "$(grep -E '^[[:space:]]*ExecStart[[:space:]]*=' "$unit")" != "$(grep '^ExecStart=' "$template")" ]; then
      echo "Custom ExecStart needs a manual upgrade; no files changed: $unit" >&2
      return 1
    fi
    for key in MEMNEST_HOST MEMNEST_PORT; do
      [ "$key" != MEMNEST_HOST ] || [ "$unit" != "$SERVICE_DIR/memnest-watch.service" ] || continue
      line="$(grep -E "^[[:space:]]*Environment[[:space:]]*=.*$key" "$unit" || true)"
      stored="${line#Environment=$key=}"
      case "$stored" in
        ''|*[!a-zA-Z0-9.:]*) echo "Unsupported $key assignment; no files changed: $unit" >&2; return 1 ;;
      esac
      [ "$line" = "Environment=$key=$stored" ] || { echo "Ambiguous $key; no files changed: $unit" >&2; return 1; }
      if [ "$key" = MEMNEST_HOST ]; then
        HOST="${MEMNEST_HOST:-$stored}"
      elif [ "$unit" = "$SERVICE_DIR/memnest.service" ]; then
        PORT="${MEMNEST_PORT:-$stored}"
        server_port="$stored"
      elif [ -n "${server_port:-}" ] && [ "$stored" != "$server_port" ]; then
        echo "Server and watcher ports differ; reconcile them before upgrading: $unit" >&2
        return 1
      elif [ -z "${server_port:-}" ]; then
        PORT="${MEMNEST_PORT:-$stored}"
      fi
    done
  done
  case "$PORT" in ''|*[!0-9]*) echo "MEMNEST_PORT must be an integer from 1 to 65535" >&2; return 1 ;; esac
  [ "${#PORT}" -le 5 ] && [ "$((10#$PORT))" -ge 1 ] && [ "$((10#$PORT))" -le 65535 ] || {
    echo "MEMNEST_PORT must be an integer from 1 to 65535" >&2; return 1;
  }
  # The packaged watcher uses IPv4 loopback. Other binds need a reviewed manual setup.
  case "$HOST" in 127.0.0.1|localhost) ;; *) echo "Packaged installs require MEMNEST_HOST=127.0.0.1 or localhost" >&2; return 1 ;; esac
  export MEMNEST_HOST="$HOST" MEMNEST_PORT="$PORT"
}
