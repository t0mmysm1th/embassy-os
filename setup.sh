#!/bin/bash
apt-get update -o Acquire::Check-Valid-Until=false -o Acquire::Check-Date=false
apt-get install -y libsecp256k1-0
apt-get install -y tor
apt-get install -y docker.io needrestart-
apt-get install -y iotop
apt-get install -y bmon
apt-get install -y libavahi-client3
apt-get autoremove -y
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
