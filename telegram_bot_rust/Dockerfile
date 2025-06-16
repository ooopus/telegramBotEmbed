# Stage 1: Build the application
FROM rust:1.75 as builder
WORKDIR /usr/src/app

# Install build essentials if needed by any crate (less likely for pure Rust but good practice)
# RUN apt-get update && apt-get install -y build-essential

# Create a dummy main.rs to cache dependencies
# This layer will only be rebuilt if Cargo.toml or Cargo.lock changes
RUN mkdir src
RUN echo "fn main() {println!(\"Dummy main for dep caching\");}" > src/main.rs

# Copy Cargo manifests and lock file (if it exists)
# The asterisk in Cargo.lock* handles cases where the file might not exist initially.
COPY Cargo.toml Cargo.lock* ./

# Build dependencies. This will build only the dummy main.rs and cache dependencies.
# If Cargo.lock is missing, this will generate one based on Cargo.toml.
# If Cargo.lock is present and out of sync, this might fail or update it.
# For reproducible builds, ensure Cargo.lock is committed and up-to-date.
RUN cargo build --release
# An alternative for just fetching dependencies if the above build is problematic:
# RUN cargo fetch

# Remove dummy main.rs before copying actual source code
RUN rm -f src/main.rs
COPY src ./src
# Copy other necessary files that might be used during build (e.g., if QA.json was embedded via include_str!)
# For now, assuming docs/QA.json is only a runtime dependency.
# COPY docs ./docs
# COPY config.example.toml .

# Build the application fully with actual source code
# This will use the cached dependencies from the previous step if Cargo.toml/Cargo.lock haven't changed.
RUN cargo build --release

# Stage 2: Create the runtime image
FROM debian:bullseye-slim
WORKDIR /app

# Create a non-root user and group for security
RUN groupadd -r appuser && useradd -r -g appuser -ms /bin/bash -d /app appuser

# Create directories for app files and set permissions
# The application itself creates cache_data if it doesn't exist, but good to have base /app/data owned by appuser
RUN mkdir -p /app/docs /app/cache_data && chown -R appuser:appuser /app

# Copy the compiled binary from the builder stage
# Ensure 'telegram_bot_rust' matches your binary name (usually the package name in Cargo.toml)
COPY --from=builder /usr/src/app/target/release/telegram_bot_rust .

# Copy runtime assets
COPY docs/QA.json ./docs/
# COPY config.example.toml ./config.toml # Optional: provide a default config.toml in the image
                                       # If you do, ensure it's also chown'd to appuser or readable.

# Ensure the binary is executable
RUN chmod +x ./telegram_bot_rust

# Switch to the non-root user
USER appuser

# Set default environment variables (can be overridden at runtime using `docker run -e VAR=value`)
ENV TELEGRAM_BOT_TOKEN=""
ENV RUST_LOG="info,telegram_bot_rust=debug" # Example: info level for all, debug for our app
ENV EMBED_API_KEY=""
ENV EMBED_API_URL="http://localhost:11434/api/embeddings" # Default to a common local ollama URL for example
ENV EMBED_MODEL="nomic-embed-text"
ENV DELETE_DELAY_SECS="60"
ENV MESSAGE_TIMEOUT_SECS="120"
ENV CACHE_DIR="/app/cache_data" # Ensure this matches a directory appuser has write access to
ENV SIMILARITY_THRESHOLD="0.75"
# Add any other environment variables your application relies on for configuration

# Command to run the application
# The application will look for config.toml in its current working directory (/app) if used,
# or primarily rely on environment variables as implemented in main.rs.
CMD ["./telegram_bot_rust"]
