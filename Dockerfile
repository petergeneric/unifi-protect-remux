FROM golang:1.15

ENV PATH=$PATH:/opt/unifi-protect-remux
WORKDIR /opt/unifi-protect-remux
COPY . /opt/unifi-protect-remux

RUN apt-get update && apt install xz-utils -y \
    && rm -rf /var/lib/apt/lists/* \
    # install ffmpeg
    && curl --fail -L --silent --show-error https://johnvansickle.com/ffmpeg/builds/ffmpeg-git-amd64-static.tar.xz > ffmpeg.tar.xz \
    && tar xf ffmpeg.tar.xz --directory /usr/local/bin/ --strip-components 1 \
    && rm ffmpeg.tar.xz \
    # install ubnt_ubvinfo
    && curl --fail -L --silent --show-error https://archive.org/download/ubnt_ubvinfo/ubnt_ubvinfo > ubnt_ubvinfo \
    && chmod u+x ubnt_ubvinfo \
    # build remux
    && make package
