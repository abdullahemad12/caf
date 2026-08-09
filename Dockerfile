FROM node:24-slim

# Install system dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    git \
    ripgrep \
    curl \
    ca-certificates \
    time \
    && rm -rf /var/lib/apt/lists/*
Run apt-get install
# Install Gemini CLI globally
RUN npm install -g @google/gemini-cli

RUN apt-get update && apt-get install -y \
    git \
    pkg-config \
    libssl-dev

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y

# Set default directory
WORKDIR /workspace

RUN time curl -s https://generativelanguage.googleapis.com

# IMPORTANT: The installed binary name is "gemini", not "gemini-cli"
ENTRYPOINT ["gemini"]
