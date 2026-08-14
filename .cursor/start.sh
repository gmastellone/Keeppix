#!/usr/bin/env bash
# Per-boot reconciliation. The Postgres process doesn't survive a snapshot or
# reboot, so bring the cluster back up and wait until it accepts connections.
# Data (the `keeppix` role and database) lives in the snapshotted data dir.
set -euo pipefail

PG_VERSION=17

sudo pg_ctlcluster "${PG_VERSION}" main start 2>/dev/null || true
for _ in $(seq 1 30); do
  sudo -u postgres pg_isready -q && break
  sleep 1
done
sudo -u postgres pg_isready
