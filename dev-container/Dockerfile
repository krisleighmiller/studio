# Reuse the upstream Hermes desktop sandbox, pinned to the inspected amd64 manifest.
FROM nousresearch/hermes-sandbox@sha256:91001712f321902e61fbd8d89d08700ab3c3b151f801bde1a46fed876f54a5ce
USER root
SHELL ["/bin/bash", "-euo", "pipefail", "-c"]
RUN apt-get update && apt-get install -y --no-install-recommends build-essential pkg-config ffmpeg gnome-keyring libsecret-tools python3-gi gir1.2-secret-1 libxtst6 && rm -rf /var/lib/apt/lists/* && rm -f /etc/sudoers.d/pn
# Match the laptop's Node runtime; checksum is verified before extraction.
RUN mkdir /tmp/node-install && cd /tmp/node-install && curl -fsSLO https://nodejs.org/dist/v26.12.0/node-v26.12.0-linux-x64.tar.xz && curl -fsSLO https://nodejs.org/dist/v26.12.0/SHASUMS256.txt && grep ' node-v26.12.0-linux-x64.tar.xz$' SHASUMS256.txt | sha256sum -c - && tar -xJf node-v26.12.0-linux-x64.tar.xz -C /usr/local --strip-components=1 && cd / && rm -rf /tmp/node-install
ENV RUSTUP_HOME=/opt/rustup
ENV PATH=/opt/cargo/bin:$PATH
RUN curl -fsSL https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init -o /tmp/rustup-init && curl -fsSL https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init.sha256 -o /tmp/rustup-init.sha256 && echo "$(cut -d ' ' -f 1 /tmp/rustup-init.sha256)  /tmp/rustup-init" | sha256sum -c - && chmod +x /tmp/rustup-init && CARGO_HOME=/opt/cargo /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain 1.97.1 && chmod -R a+rX /opt/rustup /opt/cargo && rm /tmp/rustup-init /tmp/rustup-init.sha256
COPY session.sh /opt/studio/session.sh
RUN mkdir -p /runtime /source/Studio/Desktop_App/node_modules && chmod 0777 /runtime /source/Studio/Desktop_App/node_modules && chmod 0755 /opt/studio/session.sh
USER pn
WORKDIR /home/pn
ENV DISPLAY=:99
ENV XDG_RUNTIME_DIR=/tmp/studio-runtime
ENTRYPOINT ["/opt/studio/session.sh"]
CMD ["sleep", "infinity"]
