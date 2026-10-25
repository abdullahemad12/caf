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

RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Install Gemini CLI globally
RUN npm install -g @google/gemini-cli

RUN apt-get update && apt-get install -y \
    git \
    pkg-config \
    libssl-dev \
    build-essential

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y

ENV PATH="/root/.cargo/bin:$PATH"

# Set default directory
WORKDIR /workspace

COPY gemini_entrypoint /gemini_entrypoint
RUN chmod +x /gemini_entrypoint

ENTRYPOINT ["/gemini_entrypoint"]
CMD ["gemini"]
