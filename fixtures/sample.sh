#!/usr/bin/env bash
set -euo pipefail

NAME="${1:-World}"
echo "Hello, ${NAME}!"

for i in 1 2 3; do
  echo "  Item $i"
done
