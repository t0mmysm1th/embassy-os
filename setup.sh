#!/bin/bash
apt update
apt remove -y needrestart
apt install -y libsecp256k1-0
apt install -y tor
apt install -y docker.io
apt remove -y needrestart
apt install -y iotop
apt install -y bmon
apt install -y libavahi-client3
apt autoremove -y
mkdir -p /root/volumes
mkdir -p /root/tmp/appmgr
mkdir -p /root/agent
mkdir -p /root/appmgr/tor
systemctl enable lifeline
systemctl enable agent
systemctl enable ssh
systemctl enable avahi-daemon
passwd -l root
passwd -l pi
sync
systemctl disable setup.service
reboot
