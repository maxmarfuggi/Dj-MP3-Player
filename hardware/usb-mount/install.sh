#!/bin/bash
# Install the read-only USB automount. Run as root while the system
# partition is still writable, before the image locks it.
set -euo pipefail

cd "$(dirname "$0")"
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y udisks2 dosfstools exfatprogs hfsprogs

install -d /etc/udisks2 /etc/udev/rules.d /usr/local/bin
install -m 644 mount_options.conf /etc/udisks2/mount_options.conf
install -m 644 99-djusb-automount.rules /etc/udev/rules.d/99-djusb-automount.rules
install -m 644 djusb-mount@.service /etc/systemd/system/djusb-mount@.service
install -m 755 djusb-mount /usr/local/bin/djusb-mount

# udisks reads mount_options.conf when it starts, and udev only starts the
# mount unit after systemd has loaded it. Restart first, then trigger.
systemctl daemon-reload
systemctl enable --now udisks2.service
systemctl restart udisks2.service
udevadm control --reload-rules
udevadm trigger --subsystem-match=block --action=add
