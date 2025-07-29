# --- STAGE 1: Build Frontend Assets ---
FROM node:22-alpine AS frontend-builder

WORKDIR /app/frontend

# Copy package.json and package-lock.json first to leverage Docker cache
COPY frontend/package.json frontend/package-lock.json ./

# Install frontend dependencies
RUN npm ci --prefer-offline 

# Copy frontend source code and static assets
COPY frontend/src ./src
COPY frontend/public ./public
COPY frontend/tsconfig.json ./
COPY frontend/copy-static.js ./ 

# Build the frontend
RUN npm run build

# --- STAGE 2: Build Rust Backend ---
# IMPORTANT CHANGE: Use rust:latest (Debian-based) for easier compilation
FROM rust:bookworm AS rust-builder

WORKDIR /app

# Install sqlx-cli
RUN cargo install sqlx-cli --features sqlite 

# Create DB dir and point to an absolute path for build-time DB
RUN mkdir -p /app/data
RUN touch /app/data/sqlx_build_cache.db
ENV DATABASE_URL="sqlite:///app/data/sqlx_build_cache.db"

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY login.html register.html ./
COPY migrations ./migrations

# Verification and migration steps
# Note: For sqlite, a real file needs to exist for sqlx-cli to work sometimes.
RUN echo "PWD: $(pwd)" && \
    echo "DATABASE_URL=$DATABASE_URL" && \
    ls -lah /app/data && \
    sqlx migrate run

# Build the final Rust binary
RUN cargo build --release

# IMPORTANT: Use a Debian-based slim image to match glibc from the build stage.
FROM debian:bookworm-slim AS final

# If your Rust application connects to remote databases via TLS/SSL,
# you'll need the runtime OpenSSL libraries. ca-certificates are also often needed.
RUN apt-get update && apt-get install -y \
    ca-certificates \
    # libssl3 # Uncomment this if your Rust app uses TLS for remote connections (e.g., Postgres, MySQL)
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled Rust binary from the rust-builder stage
COPY --from=rust-builder /app/target/release/web_server_axum ./web_server_axum

# Copy the built frontend assets from the frontend-builder stage
COPY --from=frontend-builder /app/public ./public

# Copy your HTML templates if they are read from Rust (and not served by ServeDir from public/)
# Ensure `home.html` is included here if your app serves it directly.
COPY login.html register.html ./

# Set environment variables for the application at runtime
ENV JWT_SECRET="your_secure_jwt_secret_here"

# Expose the port your Axum server listens on
EXPOSE 8080

# Remove the RUN command that tries to execute the binary during the build.
# This is where your application starts when the container actually runs.
CMD ["./web_server_axum"]
