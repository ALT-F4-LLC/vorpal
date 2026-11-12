#!/usr/bin/env bash
#
# lima-host — create or remove the Lima VM from the host. script/lima.sh is the
# counterpart that runs inside the VM.
#
# usage:
#   lima-host.sh clean <arch>
#   lima-host.sh create <arch> <cpus> <disk> <memory> <work-dir>

set -euo pipefail

usage() {
  echo "usage: $0 clean <arch>" >&2
  echo "       $0 create <arch> <cpus> <disk> <memory> <work-dir>" >&2
  exit 2
}

[[ "$#" -ge 2 ]] || usage
command="$1"
arch="$2"
name="vorpal-$arch"

case "$command" in
  clean)
    [[ "$#" -eq 2 ]] || usage
    limactl stop "$name" || true
    limactl delete "$name" || true
    ;;
  create)
    [[ "$#" -eq 6 ]] || usage
    cpus="$3"
    disk="$4"
    memory="$5"
    work_dir="$6"
    cd "$work_dir"
    limactl create --arch "$arch" --cpus "$cpus" --disk "$disk" --memory "$memory" --name "$name" - < lima.yaml
    limactl start "$name"
    limactl shell "$name" "$work_dir/script/lima.sh" install
    limactl stop "$name"
    limactl start "$name"
    ;;
  *)
    usage
    ;;
esac
