# Todo 服务端 todo-server 的多阶段构建
#
# 构建（本机开发/测试）：
#   docker build -t todo-server:test .
#   # 如需经过代理拉取 crates（本机沙箱环境），把代理透传为构建参数：
#   docker build -t todo-server:test \
#     --build-arg http_proxy="$http_proxy" \
#     --build-arg https_proxy="$https_proxy" \
#     --build-arg no_proxy="$no_proxy" .
#   # 代理只在 builder 阶段用于 cargo 联网，不会进入 runtime 镜像；
#   # 绝不要把 TODO_MASTER_KEY / TODO_ADMIN_PASSWORD / TODO_SYNC_TOKEN
#   # 等任何密钥写进 Dockerfile、构建参数或镜像层。
#
# 运行示例见 docker-compose.yml；镜像内不含 .env，密钥一律运行时注入。

# 基础镜像可用构建参数覆盖（默认值即生产值；仅在镜像源受限时改）：
#   docker build --build-arg RUST_IMAGE=mcr.microsoft.com/devcontainers/rust:1-bookworm \
#                --build-arg RUNTIME_IMAGE=quay.io/debian/debian:bookworm-slim .
ARG RUST_IMAGE=rust:1.99-bookworm
ARG RUNTIME_IMAGE=debian:bookworm-slim

# ---------- builder ----------
FROM ${RUST_IMAGE} AS builder

WORKDIR /app

# 仅构建期使用：cargo 拉取 crates.io 需要的代理（本机构建环境有出口代理时透传）。
# ARG 不会写进镜像层；不要在这里放任何密钥。
ARG http_proxy
ARG https_proxy
ARG no_proxy
ARG HTTP_PROXY
ARG HTTPS_PROXY
ARG NO_PROXY

# 只拷 Rust workspace 构建 todo-server 所需的文件：
# 根 Cargo.toml / Cargo.lock、两个 Rust crate，以及 src-tauri 的 manifest
# （workspace 成员清单需要它，但只做 manifest 解析，不编译 Tauri 壳）。
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY src-tauri/Cargo.toml ./src-tauri/Cargo.toml

# registry 与 target 用 BuildKit 缓存挂载：二次构建只编译改动部分。
# target 是缓存挂载、不会留在层里，所以构建完把产物拷到 /app/todo-server。
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release -p todo-server \
    && cp /app/target/release/todo-server /app/todo-server

# ---------- runtime ----------
FROM ${RUNTIME_IMAGE}

# ca-certificates：SMTP / Webhook 走 TLS 时校验对方证书需要系统根证书。
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/todo-server /usr/local/bin/todo-server

# 容器内默认值（与 SPEC §14 一致）；密钥类变量不在此设置，运行时注入。
ENV TODO_DB_PATH=/data/app.db \
    TODO_PORT=8787 \
    TODO_POLL_INTERVAL=60 \
    TODO_INSECURE_HTTP=1
# 注意：TODO_INSECURE_HTTP=1 表示容器内以裸 HTTP 监听，生产必须在前面挂
# TLS 反代（见 README「部署」一节），不要把 8787 直接暴露到公网。

VOLUME /data
EXPOSE 8787

ENTRYPOINT ["/usr/local/bin/todo-server"]
