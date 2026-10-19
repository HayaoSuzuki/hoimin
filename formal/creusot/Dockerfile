# Build on native x86_64 for acceptance measurements. No image publication.
ARG CREUSOT_BASE=ubuntu@sha256:a2d43bffc46670d7108bb3c0468c67c5557c4c92e83e1e34cbff1322ad1b94dc
FROM ${CREUSOT_BASE} AS prerequisites
ENV DEBIAN_FRONTEND=noninteractive CARGO_BUILD_JOBS=2 OPAMJOBS=2 OPAMREQUIRECHECKSUMS=true \
    OPAMROOT=/opt/creusot/opam-root OPAMSWITCH=/opt/creusot/opam-switch \
    CARGO_HOME=/opt/creusot/cargo CREUSOT_DATA_HOME=/opt/creusot/data \
    XDG_CONFIG_HOME=/opt/creusot/config XDG_CACHE_HOME=/opt/creusot/cache \
    PATH=/opt/creusot/rust/bin:/opt/creusot/opam-switch/bin:/opt/creusot/data/bin:${PATH} \
    LD_LIBRARY_PATH=/opt/creusot/rust/lib
ADD --chmod=0644 --checksum=sha256:f290e6acaf904a4121424ca3ebdd70652780707e28e8af999221786b86bb1975 https://curl.se/ca/cacert-2025-09-09.pem /etc/ssl/certs/ca-certificates.crt
# apt verifies package hashes against signed snapshot indexes. No mutable mirror.
RUN chmod 0755 /etc/ssl /etc/ssl/certs \
 && sed -i 's|http://ports.ubuntu.com/ubuntu-ports/|http://snapshot.ubuntu.com/ubuntu/20261018T000000Z/|g; s|http://archive.ubuntu.com/ubuntu/|http://snapshot.ubuntu.com/ubuntu/20261018T000000Z/|g; s|http://security.ubuntu.com/ubuntu/|http://snapshot.ubuntu.com/ubuntu/20261018T000000Z/|g' /etc/apt/sources.list.d/ubuntu.sources \
 && apt-get -o APT::Update::Error-Mode=any -o Acquire::Retries=1 -o Acquire::http::Timeout=30 update \
 && apt-get -o Acquire::Retries=1 -o Acquire::http::Timeout=30 install -y --no-install-recommends \
      ca-certificates curl python3 opam build-essential autoconf pkg-config \
      libgmp-dev zlib1g-dev libzip-dev libzmq3-dev git unzip xz-utils time \
 && rm -rf /var/lib/apt/lists/*

FROM prerequisites AS inputs
ARG TOOLCHAIN_MANIFEST=toolchain.json
COPY formal/creusot/${TOOLCHAIN_MANIFEST} /manifest.json
# Only content needed to build tools crosses this boundary; config edits reuse tools.
RUN python3 -c 'import json,pathlib; m=json.loads(pathlib.Path("/manifest.json").read_text()); pathlib.Path("/install-inputs.json").write_text(json.dumps({"artifacts":m["artifacts"],"targets":m["targets"]},sort_keys=True))'

FROM prerequisites AS rust-components
COPY --from=inputs /install-inputs.json /recipe/formal/creusot/toolchain.json
COPY formal/creusot/install-components.sh /recipe/formal/creusot/install-components.sh
RUN /usr/bin/time -v /bin/bash /recipe/formal/creusot/install-components.sh

FROM rust-components AS ocaml
COPY formal/creusot/install-ocaml.sh /recipe/formal/creusot/install-ocaml.sh
RUN /usr/bin/time -v /bin/bash /recipe/formal/creusot/install-ocaml.sh

FROM ocaml AS source-fetch
RUN apt-get -o APT::Update::Error-Mode=any -o Acquire::Retries=1 -o Acquire::http::Timeout=30 update \
 && apt-get -o Acquire::Retries=1 -o Acquire::http::Timeout=30 install -y --no-install-recommends rsync \
 && rm -rf /var/lib/apt/lists/*

FROM source-fetch AS why3
COPY formal/creusot/install-why3.sh /recipe/formal/creusot/install-why3.sh
RUN /usr/bin/time -v /bin/bash /recipe/formal/creusot/install-why3.sh

FROM why3 AS tools
COPY formal/creusot/install.sh /recipe/formal/creusot/install.sh
RUN /usr/bin/time -v /bin/bash /recipe/formal/creusot/install.sh

FROM tools AS verification
ENV PATH=/opt/creusot/rust/bin:/opt/creusot/opam-switch/_opam/bin:/opt/creusot/data/bin:${PATH}
ARG TOOLCHAIN_MANIFEST=toolchain.json
COPY formal/creusot /recipe/formal/creusot
COPY formal/creusot/${TOOLCHAIN_MANIFEST} /recipe/formal/creusot/toolchain.json
COPY tools/creusot_environment.py /recipe/tools/creusot_environment.py
RUN /usr/bin/time -v /bin/bash /recipe/formal/creusot/configure.sh
WORKDIR /workspace
CMD ["python3", "/recipe/tools/creusot_environment.py", "check-installation", "--manifest", "/recipe/formal/creusot/toolchain.json"]
