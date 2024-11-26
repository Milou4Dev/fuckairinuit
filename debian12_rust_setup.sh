#!/bin/bash

set -euo pipefail

[[ $EUID -ne 0 ]] && {
    echo "This script must be run as root"
    exit 1
}

SERVICE_USER="rustservice"
APP_DIR="/opt/rustapp"
GITHUB_REPO="https://github.com/Milou4Dev/fuckairinuit"

apt-get update && apt-get upgrade -y
apt-get install -y curl build-essential pkg-config libssl-dev git

useradd -r -m -s /bin/bash $SERVICE_USER

su - $SERVICE_USER -c 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y'
su - $SERVICE_USER -c '. $HOME/.cargo/env'

mkdir -p $APP_DIR
git clone $GITHUB_REPO $APP_DIR
chown -R $SERVICE_USER:$SERVICE_USER $APP_DIR
chmod 750 $APP_DIR

cat >/etc/systemd/system/rustapp.service <<'EOL'
[Unit]
Description=Rust Application Service
After=network.target
StartLimitIntervalSec=0

[Service]
Type=simple
User=rustservice
Group=rustservice
WorkingDirectory=/opt/rustapp
Environment="PATH=/home/rustservice/.cargo/bin:/usr/local/bin:/usr/bin:/bin"
ExecStart=/bin/bash -c 'source /home/rustservice/.cargo/env && exec /home/rustservice/.cargo/bin/cargo run --release'
Restart=always
RestartSec=1
StartLimitBurst=0
LimitNOFILE=65535
TimeoutStartSec=0
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
PrivateTmp=true
PrivateDevices=true
ProtectKernelTunables=true
ProtectControlGroups=true
RestrictAddressFamilies=AF_INET AF_INET6
RestrictNamespaces=true
SystemCallFilter=@system-service
SystemCallErrorNumber=EPERM
MemoryDenyWriteExecute=true

[Install]
WantedBy=multi-user.target
EOL

systemctl daemon-reload
systemctl enable rustapp
systemctl start rustapp

exit 0
