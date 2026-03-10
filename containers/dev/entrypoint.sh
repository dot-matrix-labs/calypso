#!/bin/bash
set -e

# Start Docker daemon (Docker-in-Docker)
if [ ! -f /var/run/docker.pid ]; then
    dockerd &>/var/log/dockerd.log &
    # Wait for daemon to be ready
    timeout 30 sh -c 'until docker info >/dev/null 2>&1; do sleep 1; done'
fi

# Inject authorized SSH keys from environment variable if provided
if [ -n "${SSH_AUTHORIZED_KEYS}" ]; then
    mkdir -p /home/agent/.ssh
    echo "${SSH_AUTHORIZED_KEYS}" > /home/agent/.ssh/authorized_keys
    chmod 600 /home/agent/.ssh/authorized_keys
    chown -R agent:agent /home/agent/.ssh
fi

# Start SSH daemon
/usr/sbin/sshd

# If a command was passed, run it as agent user; otherwise keep container alive
if [ "$#" -gt 0 ]; then
    exec su -c "$*" agent
else
    exec tail -f /dev/null
fi
