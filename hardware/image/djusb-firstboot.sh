#!/bin/bash
# Runs once, as root, on the first boot of a card prepared by prepare-boot.ps1.
# The system partition is still writable here. Settings go in an ext4 image on
# the boot partition so they survive the read-only root.

set -u
LOG=/var/log/djusb-firstboot.log
mkdir -p /var/log
exec > >(tee -a "$LOG") 2>&1

if [ -f /boot/firmware/djusb-firstboot.done ]; then
  exit 0
fi

echo "DJ USB first boot"

systemctl enable --now ssh || true

echo "Waiting for the network"
online=0
for _ in $(seq 1 60); do
  if getent hosts archive.raspberrypi.com >/dev/null 2>&1 \
    || getent hosts deb.debian.org >/dev/null 2>&1; then
    online=1
    break
  fi
  sleep 2
done
if [ "$online" -ne 1 ]; then
  echo "No network. The install cannot continue."
  exit 1
fi

set -e
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y cage wlr-randr

install -m 755 /boot/firmware/djusb-device /usr/local/bin/djusb-device
install -m 755 /boot/firmware/djusb-kiosk /usr/local/bin/djusb-kiosk
install -m 644 /boot/firmware/djusb.service /etc/systemd/system/djusb.service

image=/boot/firmware/djusb-data.img
if [ ! -s "$image" ]; then
  dd if=/dev/zero of="$image" bs=1M count=32 status=none
  mkfs.ext4 -F -L djusb-data "$image"
fi
mkdir -p /var/lib/djusb
modprobe loop || true
if ! grep -q 'djusb-data.img' /etc/fstab; then
  echo '/boot/firmware/djusb-data.img /var/lib/djusb ext4 loop,nofail,x-systemd.requires-mounts-for=/boot/firmware 0 2' >> /etc/fstab
fi
mount /var/lib/djusb
for group in gpio video render input audio; do
  getent group "$group" >/dev/null || groupadd "$group"
  usermod -aG "$group" admin
done
chown admin:admin /var/lib/djusb
chmod 775 /var/lib/djusb

cmdline=/boot/firmware/cmdline.txt
grep -q 'panel_orientation=left_side_up' "$cmdline" || sed -i '1s/$/ video=DSI-1:panel_orientation=left_side_up/' "$cmdline"
grep -q 'quiet splash' "$cmdline" || sed -i '1s/$/ quiet splash/' "$cmdline"

systemctl daemon-reload
systemctl enable djusb.service
raspi-config nonint do_boot_behaviour B1 || true

if [ -f /boot/firmware/usb-mount/install.sh ]; then
  echo "Installing read-only USB automount"
  bash /boot/firmware/usb-mount/install.sh
fi

if ! raspi-config nonint do_overlayfs 0; then
  echo "Could not lock the system partition."
  exit 1
fi

touch /boot/firmware/djusb-firstboot.done
sync
reboot
