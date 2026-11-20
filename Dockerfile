FROM rust:1.85-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p brainmesh-cli

FROM debian:bookworm-slim
RUN useradd -m -u 10001 brainmesh && mkdir -p /data && chown brainmesh:brainmesh /data
COPY --from=builder /app/target/release/brainmesh /usr/local/bin/brainmesh
USER brainmesh
EXPOSE 8080
ENTRYPOINT ["brainmesh"]
CMD ["server", "start", "--addr", "0.0.0.0:8080"]
