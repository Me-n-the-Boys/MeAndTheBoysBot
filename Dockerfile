FROM rust:alpine as builder
RUN apk add musl-dev
WORKDIR /rust-dc-bot
#build layer with dummy project first, to cache dependencies
ADD Cargo.* .
RUN \
    mkdir src && \
    echo 'fn main() {}' > src/main.rs && \
    cargo build --release && \
    rm -Rvf src
#actually build the application now
ADD src/ src/
RUN touch src/main.rs && cargo build --release

#FROM alpine:latest
FROM scratch
COPY --from=builder /rust-dc-bot/target/release/untitled /untitled
WORKDIR "/data"
CMD ["/untitled"]
