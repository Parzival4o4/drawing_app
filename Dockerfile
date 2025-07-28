# --- STAGE 1: Build Frontend Assets ---
FROM node:22-alpine AS frontend-builder

WORKDIR /app/frontend

# Copy package.json and package-lock.json first to leverage Docker cache
COPY frontend/package.json frontend/package-lock.json ./

# Install frontend dependencies
# 'ci' is better for CI/CD, ensures clean install
RUN npm ci --prefer-offline 

# Copy frontend source code and static assets
COPY frontend/src ./src
COPY frontend/public ./public
COPY frontend/tsconfig.json ./
# Copy the helper script
COPY frontend/copy-static.js ./ 

# Build the frontend (tsc compiles, then copy-static.js moves files)
RUN npm run build

# --- STAGE 2: Build Rust Backend ---
FROM rust:alpine AS rust-builder

WORKDIR /app

# Install musl-dev for static compilation (important for alpine base)
RUN apk add --no-cache musl-dev

# Copy Cargo.toml and Cargo.lock first to leverage Docker cache for dependencies
COPY Cargo.toml Cargo.lock ./

# Copy your actual Rust source code
COPY src ./src
# Copy your HTML templates if they are read by Rust
COPY login.html register.html home.html ./

# Build the final Rust binary
RUN cargo build --release

# --- STAGE 3: Final Production Image ---
FROM alpine:latest AS final

# Set default user and group for security (optional but good practice)
# RUN addgroup -S appgroup && adduser -S appuser -G appgroup
# USER appuser

WORKDIR /app

# Copy the compiled Rust binary from the rust-builder stage
# Replace 'web_server_axum' with your binary name
COPY --from=rust-builder /app/target/release/web_server_axum ./web_server_axum

# Copy the built frontend assets from the frontend-builder stage
# This correctly points to /app/public after frontend build
COPY --from=frontend-builder /app/frontend/../public ./public

# Copy your HTML templates if they are part of the served content, and not frontend assets
# This is redundant if already in public/, but if you read them from Rust, they need to be here.
COPY login.html register.html home.html ./

# Set environment variables for the application (e.g., JWT secret)
# !!! IMPORTANT: Replace with a strong secret or pass at runtime
ENV JWT_SECRET="your_secure_jwt_secret_here"

# Expose the port your Axum server listens on
EXPOSE 3000

# Command to run your application
CMD ["./web_server_axum"]