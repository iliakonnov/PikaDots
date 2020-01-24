FROM rustlang/rust:nightly as builder

WORKDIR /app_src
COPY Cargo.* Rocket.toml ./
COPY src/ ./src/
# ARG VERSION=12e7e2f4290927a7935a7b4c7e248df98b1b4c62
# RUN wget https://github.com/iliakonnov/PikaDots/archive/$VERSION.tar.gz -O sources.tar.gz 2> /dev/null \
#       && tar -xf sources.tar.gz && rm sources.tar.gz \
#       && mv PikaDots-*/.[!.]* ./ && mv PikaDots-*/* ./ \
#       && rmdir PikaDots-* \
#       && cargo build --release
RUN cargo build --release

FROM debian:buster-slim
RUN groupadd -r user && useradd --no-log-init -r -g user user
USER user
COPY ./data.dat ./index.idx /
COPY --from=builder /app_src/target/release/pikadots /usr/local/bin/pikadots
EXPOSE 8000
CMD ["pikadots", "serve", "--data" ,"/data.dat", "--index", "/index.idx"]
