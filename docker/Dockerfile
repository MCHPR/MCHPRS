FROM rust:slim

RUN apt-get update \
    && apt-get install -y \
    git pkg-config libssl-dev

RUN git clone https://github.com/MCHPR/MCHPRS.git
WORKDIR ./MCHPRS
RUN cargo install --path . \
    && cargo clean

VOLUME ["/data"]
WORKDIR /data

CMD ["mchprs"]