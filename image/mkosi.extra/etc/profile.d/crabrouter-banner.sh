#!/bin/sh
# Display concise physical-appliance status & recovery commands on interactive root shells.
case "$-" in
  *i*)
    printf '\n=== CrabRouter Appliance Console ===\n'
    printf 'Service : crabrouterd=%s\n' "$(systemctl is-active crabrouterd.service 2>/dev/null || echo unknown)"
    printf 'WebUI   : http://192.168.10.1:8080 (or active LAN IP)\n'
    printf 'NICs    :\n'
    ip -brief link show 2>/dev/null | sed 's/^/  /' || true
    printf 'Tips    : crabctl interfaces | crabctl pin-nics --apply | crabctl disks\n\n'
    ;;
esac
