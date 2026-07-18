# ==============================================================================
# Unified Processor Service - Dockerfile
# ==============================================================================
# Multi-stage build for Rust + Python hybrid service
# Port: 8090 (primary), 3019, 3001 (compatibility)
# ==============================================================================

# ==============================================================================
# Stage 1: Rust builder
# ==============================================================================
FROM debian:bookworm-slim AS rust-builder

# Install all build-time dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    ca-certificates \
    pkg-config \
    libssl-dev \
    libcurl4-openssl-dev \
    zlib1g-dev \
    python3-dev \
    python3-pip \
    cmake \
    build-essential \
    librdkafka-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Rust via rustup to guarantee latest stable compiler
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /app

# ---------------------------------------------------------------------------
# Dependency caching layer
# ---------------------------------------------------------------------------
COPY Cargo.toml Cargo.lock* ./

# Build a minimal stub that mirrors the actual lib.rs module layout so cargo
# can compile all crate dependencies without the real source files.
RUN mkdir -p \
        api \
        src/core \
        src/processors \
        src/infra \
        src/graph \
        src/utils && \
    echo 'fn main() {}' > api/index.rs && \
    printf 'pub mod core;\npub mod processors;\npub mod infra;\npub mod graph;\n' > src/lib.rs && \
    touch \
        src/core/mod.rs \
        src/processors/mod.rs \
        src/infra/mod.rs \
        src/graph/mod.rs

# Cache dependencies
RUN cargo build --release 2>/dev/null; \
    cargo clean -p unified-processor 2>/dev/null; \
    true

# ---------------------------------------------------------------------------
# Real build
# ---------------------------------------------------------------------------
RUN rm -rf src/* api/*
COPY src/ ./src/
COPY api/ ./api/

# Force Cargo to invalidate the cache by updating timestamps
RUN touch src/lib.rs api/index.rs && cargo build --release

# ==============================================================================
# Stage 2: Python dependencies
# ==============================================================================
FROM debian:bookworm-slim AS python-builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 \
    python3-venv \
    python3-pip \
    python3-dev \
    build-essential \
    libgl1 \
    libglib2.0-0 \
    libsm6 \
    libxext6 \
    libxrender-dev \
    libgomp1 \
    libpoppler-dev \
    poppler-utils \
    tesseract-ocr \
    tesseract-ocr-eng \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Create a virtual environment to safely install Python packages
RUN python3 -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

COPY pyproject.toml README.md* ./
COPY src/utils/ ./src/utils/
RUN pip install --no-cache-dir .
# ==============================================================================
# Stage 3: Runtime image
# ==============================================================================
FROM debian:bookworm-slim AS runtime

# Install ALL shared libraries that the Rust binary links against at runtime.
# We install python3-dev to guarantee 100% identical shared libraries as the rust-builder stage.
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 \
    python3-dev \
    dumb-init \
    curl \
    ca-certificates \
    libpq5 \
    libssl3 \
    librdkafka1 \
    libgomp1 \
    libglib2.0-0 \
    && rm -rf /var/lib/apt/lists/*

# Guarantee the linker can find the shared library
ENV LD_LIBRARY_PATH="/usr/lib/x86_64-linux-gnu:/usr/local/lib:$LD_LIBRARY_PATH"

# SECURITY: Create a non-root user and group
RUN groupadd -r appgroup && useradd -r -g appgroup appuser

WORKDIR /app

# Copy Python virtual environment from the builder stage
COPY --from=python-builder /opt/venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"
ENV VIRTUAL_ENV="/opt/venv"

# Copy the compiled Rust binary and set ownership
COPY --from=rust-builder --chown=appuser:appgroup /app/target/release/unified-processor /usr/local/bin/unified-processor

# Copy application source so pyo3 can import the Python document processor
COPY --chown=appuser:appgroup src/ ./src/

# Ensure the appuser owns the working directory
RUN chown -R appuser:appgroup /app

ENV PORT=10000

# SECURITY: Switch to the non-root user
USER appuser



EXPOSE 10000

ENTRYPOINT ["dumb-init", "--"]
CMD ["unified-processor"]
