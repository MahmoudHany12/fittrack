FROM rust:1-slim-bookworm AS build
WORKDIR /app
COPY Cargo.* ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 app
COPY --from=build /app/target/release/fittrack-api /usr/local/bin/fittrack-api
USER app
EXPOSE 8080
CMD ["fittrack-api"]
