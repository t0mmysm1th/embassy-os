dbPath = "root/agent/start9_agent.sqlite3"

agentDataDirectory = "/root/agent"

agentTmpDirectory = "/root/agent/tmp"

iconBasePath = "/root/agent/icons"

nginxConfig = "/etc/nginx/nginx.conf"

journaldConfig = "/etc/systemd/journald.conf"

nginxSitesAvailable = "/etc/nginx/sites-available"

nginxSitesEnabled = "/etc/nginx/sites-enabled"

sshKeysDirectory = "/home/pi/.ssh"

sshKeysFilePath = sshKeysDirectory <> "authorized_keys"

agentTorHiddenServiceDirectory = "/var/lib/tor/agent"

serverNamePath = "/root/agent/name.txt"

altRegistryUrlPath = "/root/agent/alt_registry_url.txt"

sessionSigningKeyPath = "/root/agent/start9.aes"

rootCaDirectory = agentDataDirectory <> "/ca"

sslDirectory = "/etc/nginx/ssl"
