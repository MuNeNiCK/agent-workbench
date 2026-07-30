FROM alpine@sha256:14358309a308569c32bdc37e2e0e9694be33a9d99e68afb0f5ff33cc1f695dce AS lean

RUN apk add --no-cache \
      bash build-base ccache clang cmake curl git gmp-dev libuv-dev linux-headers \
      llvm make ninja samurai zstd-dev

WORKDIR /opt
RUN git clone --depth 1 --branch v4.30.0 https://github.com/leanprover/lean4.git \
 && test "$(git -C lean4 rev-parse HEAD)" = \
      "d024af099ca4bf2c86f649261ebf59565dc8c622"

WORKDIR /opt/lean4
RUN cmake --preset release -DUSE_MIMALLOC=OFF \
      -DCMAKE_C_COMPILER=clang -DCMAKE_CXX_COMPILER=clang++ \
 && make -C build/release -j2
RUN apk add --no-cache \
      file gmp-static libc++-static libunwind-static libuv-static
RUN ln -sf libgmp.a /usr/lib/libgmp.so

ENV PATH="/opt/lean4/build/release/stage1/bin:${PATH}"
ENV LEAN_SYSROOT="/opt/lean4/build/release/stage1"

FROM lean AS build
WORKDIR /src
COPY . .
RUN lake -KstaticRelease=true build \
 && test "$(.lake/build/bin/agent-workbench --version)" = \
      "agent-workbench 0.2.3" \
 && file .lake/build/bin/agent-workbench | grep -F "statically linked"

FROM build AS stripped
RUN strip .lake/build/bin/agent-workbench

FROM scratch AS artifact
COPY --from=stripped /src/.lake/build/bin/agent-workbench /agent-workbench

FROM lean AS formal-tool
COPY release/formal-tool-exec.sh /tmp/formal-tool-exec.sh
COPY scripts/test-formal-tool-asset.sh /tmp/test-formal-tool-asset.sh
RUN set -eu; \
    tool_root=/opt/agent-workbench-formal-tool; \
    stage=/opt/lean4/build/release/stage1; \
    mkdir -p "$tool_root/bin" "$tool_root/lib"; \
    cp -R "$stage/lib/." "$tool_root/lib/"; \
    find "$tool_root/lib" -type f \
      \( -name '*.a' -o -name '*.bc' -o -name '*.c' -o -name '*.export' \
         -o -name '*.hash' -o -name '*.ilean' -o -name '*.o' -o -name '*.rsp' \
         -o -name '*.trace' \) -delete; \
    cp "$stage/bin/lean" "$stage/bin/lake" "$tool_root/bin/"; \
    if test -x "$stage/bin/cadical"; then cp "$stage/bin/cadical" "$tool_root/bin/"; fi; \
    for tool in lean lake; do \
      mv "$tool_root/bin/$tool" "$tool_root/bin/.$tool.real"; \
      cp /tmp/formal-tool-exec.sh "$tool_root/bin/$tool"; \
      chmod +x "$tool_root/bin/$tool"; \
    done; \
    cp -L /lib/ld-musl-x86_64.so.1 \
      /usr/lib/libgcc_s.so.1 /usr/lib/libgmp.so.10 \
      /usr/lib/libstdc++.so.6 /usr/lib/libuv.so.1 \
      "$tool_root/lib/"; \
    printf '%s\n' 'leanprover/lean4:v4.30.0' > "$tool_root/lean-toolchain"; \
    printf '%s\n' 'd024af099ca4bf2c86f649261ebf59565dc8c622' \
      > "$tool_root/SOURCE_COMMIT"; \
    (cd "$tool_root" && find . -type f ! -name MANIFEST.sha256 -print0 \
      | sort -z | xargs -0 sha256sum > MANIFEST.sha256); \
    "$tool_root/bin/lean" --version; \
    "$tool_root/bin/lake" --version; \
    /tmp/test-formal-tool-asset.sh "$tool_root"

FROM formal-tool AS formal-tool-archive-build
ARG SOURCE_DATE_EPOCH
RUN set -eu; \
    test -n "$SOURCE_DATE_EPOCH"; \
    apk add --no-cache gzip tar; \
    mkdir -p /out; \
    tar --sort=name --mtime="@$SOURCE_DATE_EPOCH" \
      --owner=0 --group=0 --numeric-owner \
      -C /opt -cf - agent-workbench-formal-tool \
      | gzip -n > /out/agent-workbench-formal-tool.tar.gz

FROM scratch AS formal-tool-archive
COPY --from=formal-tool-archive-build \
  /out/agent-workbench-formal-tool.tar.gz \
  /agent-workbench-formal-tool.tar.gz
