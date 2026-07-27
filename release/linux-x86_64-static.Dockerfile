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
      "agent-workbench 0.2.2" \
 && file .lake/build/bin/agent-workbench | grep -F "statically linked"

FROM build AS tested
RUN apk add --no-cache nodejs python3 sqlite \
 && .lake/build/bin/kernel-laws \
 && .lake/build/bin/storage-laws \
 && .lake/build/bin/workflow-laws \
 && .lake/build/bin/cli-laws \
 && strip .lake/build/bin/agent-workbench

FROM scratch AS artifact
COPY --from=tested /src/.lake/build/bin/agent-workbench /agent-workbench
