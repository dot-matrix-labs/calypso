FROM ubuntu:24.04

LABEL org.opencontainers.image.source="https://github.com/dot-matrix-labs/calypso"
LABEL org.opencontainers.image.description="Calypso developer container — agent CLIs, build tools, Docker-in-Docker"
LABEL org.opencontainers.image.licenses="MIT"

ENV DEBIAN_FRONTEND=noninteractive

# ── System packages ───────────────────────────────────────────────────────────
RUN apt-get update && apt-get install -y --no-install-recommends \
    # Core utilities
    bash \
    curl \
    wget \
    ca-certificates \
    gnupg \
    lsb-release \
    unzip \
    zip \
    jq \
    # Version control
    git \
    openssh-client \
    # Session management
    tmux \
    # Process tools
    procps \
    htop \
    # Build essentials (for native addons)
    build-essential \
    python3 \
    # Playwright OS dependencies
    libnss3 \
    libatk1.0-0 \
    libatk-bridge2.0-0 \
    libcups2 \
    libdrm2 \
    libxkbcommon0 \
    libxcomposite1 \
    libxdamage1 \
    libxfixes3 \
    libxrandr2 \
    libgbm1 \
    libasound2t64 \
    fonts-liberation \
    # SSH server (for IDE remote attach)
    openssh-server \
    && rm -rf /var/lib/apt/lists/*

# ── Docker CLI + daemon (Docker-in-Docker) ────────────────────────────────────
RUN install -m 0755 -d /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/ubuntu/gpg \
       | gpg --dearmor -o /etc/apt/keyrings/docker.gpg \
    && chmod a+r /etc/apt/keyrings/docker.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
       https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable" \
       > /etc/apt/sources.list.d/docker.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
       docker-ce \
       docker-ce-cli \
       containerd.io \
       docker-buildx-plugin \
    && rm -rf /var/lib/apt/lists/*

# ── GitHub CLI ────────────────────────────────────────────────────────────────
RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
       | dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \
    && chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] \
       https://cli.github.com/packages stable main" \
       > /etc/apt/sources.list.d/github-cli.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends gh \
    && rm -rf /var/lib/apt/lists/*

# ── Node.js (LTS) via NodeSource — installed before Bun since npm installs Bun ─
RUN curl -fsSL https://deb.nodesource.com/setup_lts.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# ── Bun — installed via npm to avoid bun.sh/install script argument parsing ───
RUN npm install -g bun \
    && ln -sf "$(npm root -g)/bun/bin/bun" /usr/local/bin/bun \
    && ln -sf "$(npm root -g)/bun/bin/bunx" /usr/local/bin/bunx

# ── Claude CLI ────────────────────────────────────────────────────────────────
RUN npm install -g @anthropic-ai/claude-code

# ── SSH server setup ──────────────────────────────────────────────────────────
RUN mkdir -p /var/run/sshd \
    && sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin no/' /etc/ssh/sshd_config \
    && sed -i 's/#PubkeyAuthentication yes/PubkeyAuthentication yes/' /etc/ssh/sshd_config \
    && sed -i 's/#PasswordAuthentication yes/PasswordAuthentication no/' /etc/ssh/sshd_config

# ── Create non-root agent user ────────────────────────────────────────────────
RUN useradd -m -s /bin/bash -G docker agent \
    && mkdir -p /home/agent/.ssh \
    && chmod 700 /home/agent/.ssh

# ── Workspace ─────────────────────────────────────────────────────────────────
WORKDIR /workspace
RUN chown agent:agent /workspace

# ── Entrypoint ────────────────────────────────────────────────────────────────
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

EXPOSE 22

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
