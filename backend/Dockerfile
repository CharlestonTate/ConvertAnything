FROM rust:1.83-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        ffmpeg \
        libvips-tools \
        libreoffice \
        fonts-dejavu-core \
        default-jre-headless \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /tmp/convert

ENV CONVERT_DATA_DIR=/tmp/convert
ENV PORT=8080
ENV HOME=/tmp
WORKDIR /app
COPY --from=builder /app/target/release/convert-anything /usr/local/bin/convert-anything
EXPOSE 8080
CMD ["convert-anything"]
