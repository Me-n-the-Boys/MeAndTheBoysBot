FROM rust:alpine AS builder
RUN apk add musl-dev
WORKDIR /rust-dc-bot
COPY Cargo.lock Cargo.toml dummy.rs ./
RUN mkdir .cargo && cargo vendor > .cargo/config.toml && cargo build --bin dummy --release
COPY src/ src/
RUN cargo build --release

FROM scratch
COPY --from=builder /rust-dc-bot/target/release/me-and-the-boys-dcbot /me-and-the-boys-dcbot
WORKDIR "/data"
EXPOSE 8000
CMD ["/me-and-the-boys-dcbot"]
