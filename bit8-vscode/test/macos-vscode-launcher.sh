#!/bin/sh
set -eu

if [ -z "${BIT8_VSCODE_APP_PATH:-}" ]; then
  echo "Set BIT8_VSCODE_APP_PATH to the installed Visual Studio Code.app path." >&2
  exit 2
fi

exec /usr/bin/open -n -W -a "$BIT8_VSCODE_APP_PATH" --args "$@"
