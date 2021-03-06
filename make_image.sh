#!/bin/bash
arch=$(uname -m)
if [[ $arch == armv7l ]]; then
    dev_target="target"
else 
    dev_target="target/armv7-unknown-linux-musleabihf"
fi
mv buster.img embassy.img
product_key=$(cat product_key)
loopdev=$(losetup -f -P embassy.img --show)
root_mountpoint="/mnt/start9-${product_key}-root"
boot_mountpoint="/mnt/start9-${product_key}-boot"
mkdir -p "${root_mountpoint}"
mkdir -p "${boot_mountpoint}"
mount "${loopdev}p2" "${root_mountpoint}"
mount "${loopdev}p1" "${boot_mountpoint}"
mkdir -p "${root_mountpoint}/root/agent"
mkdir -p "${root_mountpoint}/etc/docker"
mkdir -p "${root_mountpoint}/home/pi/.ssh"
echo -n "" > "${root_mountpoint}/home/pi/.ssh/authorized_keys"
chown -R pi:pi "${root_mountpoint}/home/pi/.ssh"
echo -n "" > "${boot_mountpoint}/ssh"
echo "${product_key}" > "${root_mountpoint}/root/agent/product_key"
echo -n "start9-" > "${root_mountpoint}/etc/hostname"
echo -n "${product_key}" | shasum -t -a 256 | cut -c1-8 >> "${root_mountpoint}/etc/hostname"
cat "${root_mountpoint}/etc/hosts" | grep -v "127.0.1.1" > "${root_mountpoint}/etc/hosts.tmp"
echo -ne "127.0.1.1\tstart9-" >> "${root_mountpoint}/etc/hosts.tmp"
echo -n "${product_key}" | shasum -t -a 256 | cut -c1-8 >> "${root_mountpoint}/etc/hosts.tmp"
mv "${root_mountpoint}/etc/hosts.tmp" "${root_mountpoint}/etc/hosts"
cp agent/dist/agent "${root_mountpoint}/usr/local/bin/agent"
chmod 700 "${root_mountpoint}/usr/local/bin/agent"
cp "appmgr/${dev_target}/release/appmgr" "${root_mountpoint}/usr/local/bin/appmgr"
chmod 700 "${root_mountpoint}/usr/local/bin/appmgr"
cp "lifeline/${dev_target}/release/lifeline" "${root_mountpoint}/usr/local/bin/lifeline"
chmod 700 "${root_mountpoint}/usr/local/bin/lifeline"
cp docker-daemon.json "${root_mountpoint}/etc/docker/daemon.json"
cp setup.sh "${root_mountpoint}/root/setup.sh"
chmod 700 "${root_mountpoint}/root/setup.sh"
cp setup.service "${root_mountpoint}/etc/systemd/system/setup.service"
ln -s  /etc/systemd/system/setup.service "${root_mountpoint}/etc/systemd/system/getty.target.wants/setup.service"
cp lifeline/lifeline.service "${root_mountpoint}/etc/systemd/system/lifeline.service"
cp agent/config/agent.service "${root_mountpoint}/etc/systemd/system/agent.service"
cat "${boot_mountpoint}/config.txt" | grep -v "dtoverlay=" > "${boot_mountpoint}/config.txt.tmp"
echo "dtoverlay=pwm-2chan" >> "${boot_mountpoint}/config.txt.tmp"
mv "${boot_mountpoint}/config.txt.tmp" "${boot_mountpoint}/config.txt"
umount "${root_mountpoint}"
rm -r "${root_mountpoint}"
umount "${boot_mountpoint}"
rm -r "${boot_mountpoint}"
losetup -d ${loopdev}
echo "DONE! Here is your EmbassyOS key: ${product_key}"