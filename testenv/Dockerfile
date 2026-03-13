# CentOS 5.11 + sshd — remote-merge 負荷テスト環境
#
# CentOS 5 は EOL のため yum が使えない（vault.centos.org が HTTPS 強制 +
# CentOS 5 の SSL ライブラリが TLS 1.2 未対応）。
# そのため openssh の RPM をホスト側で事前ダウンロードし、rpm -i で直接インストールする。
#
# ホストカーネルは WSL2 (6.6.x) なので ARG_MAX 等はホスト依存だが、
# ユーザーランド（古い bash 3.x, coreutils 5.x）はネイティブ CentOS 5。
#
# 前提: WSL2 で kernelCommandLine = vsyscall=emulate が必要

FROM centos:5

# ── openssh RPM をホストからコピーしてインストール ──
# RPM は vault.centos.org/5.11/os/x86_64/CentOS/ からダウンロード済み
COPY rpms/ /tmp/rpms/
RUN rpm -ivh --nosignature /tmp/rpms/*.rpm && \
    rm -rf /tmp/rpms

# ── SSH サーバ設定 ──
# sshd_config は COPY で上書き（sed -i が segfault するため）
RUN mkdir -p /var/run/sshd && \
    ssh-keygen -t rsa -f /etc/ssh/ssh_host_rsa_key -N '' && \
    ssh-keygen -t dsa -f /etc/ssh/ssh_host_dsa_key -N ''

COPY sshd_config /etc/ssh/sshd_config

# ── テストユーザー作成 ──
# passwd パッケージはベースイメージにプリインストール済み
RUN useradd -m testuser && \
    echo 'testuser:testpass' | chpasswd

# ── SSH 公開鍵を組み込み ──
# CentOS 5 の openssh 4.3 は ed25519 未対応のため RSA 鍵を使用
COPY .ssh/id_rsa.pub /home/testuser/.ssh/authorized_keys
RUN chmod 700 /home/testuser/.ssh && \
    chmod 600 /home/testuser/.ssh/authorized_keys && \
    chown -R testuser:testuser /home/testuser/.ssh

# ── テストデータ配置先 ──
RUN mkdir -p /srv/testdata && \
    chown testuser:testuser /srv/testdata

EXPOSE 22

CMD ["/usr/sbin/sshd", "-D"]
