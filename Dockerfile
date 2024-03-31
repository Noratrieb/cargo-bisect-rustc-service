FROM rust:1.77 as build

RUN rustup toolchain install nightly
RUN rustup default nightly
RUN rustup target add x86_64-unknown-linux-musl

RUN apt-get update
RUN apt-get install musl-tools -y

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN mkdir src
RUN echo "fn main() {}" > src/main.rs

RUN cargo build --release

COPY src ./src
COPY index.html index.html

# now rebuild with the proper main
RUN touch src/main.rs
RUN cargo build --release

### RUN
FROM rust:1.77

RUN cargo install cargo-bisect-rustc

WORKDIR /app

# random user
RUN useradd --create-home bisector
USER bisector

# this server listens on port 4000 and 4001 (metrics)
EXPOSE 4000
EXPOSE 4001

COPY --from=build /app/target/release/cargo-bisect-rustc-service cargo-bisect-rustc-service

CMD ["/app/cargo-bisect-rustc-service"]
