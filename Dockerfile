FROM rust:1.95.0-slim-trixie
WORKDIR /AnonCred

RUN apt-get update && apt-get install -y --no-install-recommends iproute2 python3 python3-matplotlib

COPY . .

RUN cargo build --release


