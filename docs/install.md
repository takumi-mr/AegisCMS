[← Back to the documentation index](README.md)

**English** | [日本語](install.ja.md)

# Installing AegisCMS

Choose how to run AegisCMS: [Docker](#1-docker), a [Linux binary](#2-linux-binary), or a [Windows binary](#3-windows-binary). For the required environment, see [System Requirements](../README.md#system-requirements). All of them start on SQLite with nothing else to install.

After starting, [create an administrator](#4-create-an-administrator). Until you do, nobody can log in.

## 1. Docker

```bash
docker run -d --name aegiscms -p 8080:8080 -v aegis-data:/app/data ghcr.io/takumi-mr/aegiscms:latest
```

- Open `http://localhost:8080/` for the site and `http://localhost:8080/admin/` for Admin Studio.
- The database and uploaded files are in `/app/data`. Keep it in a volume (`-v aegis-data:/app/data`) so they survive when the container is replaced.
- View logs with `docker logs aegiscms`. Stop with `docker stop aegiscms`.
- The image supports `linux/amd64` and `linux/arm64`; Docker picks the right one.

## 2. Linux Binary

1. Download `aegiscms-<version>-x86_64-unknown-linux-gnu.tar.gz` (or `aarch64-unknown-linux-gnu` for arm64) from [Releases](https://github.com/takumi-mr/AegisCMS/releases/latest). Check it with the `.sha256` file next to it.
2. Extract it and start it **from the extracted folder** (settings, schemas, themes, and plugins are found by relative path):

```bash
sha256sum -c aegiscms-*.tar.gz.sha256
tar xzf aegiscms-*.tar.gz
cd aegiscms-*/
./aegis-server run
```

The binary needs glibc 2.35 or later (Ubuntu 22.04+, Debian 12+).

The default settings (`config/default.toml`) listen on `127.0.0.1:8080` only, so other machines cannot reach it. Put a reverse proxy in front ([Publishing](publish.md)), or set `host = "0.0.0.0"` under `[server]` if the network is trusted. Change the port with `--port` or the `PORT` environment variable.

The database (`aegis.db`) and uploads (`data/`) are created in the current folder.

### Running as a service (systemd)

```ini
# /etc/systemd/system/aegiscms.service
[Unit]
Description=AegisCMS
After=network.target

[Service]
User=aegis
WorkingDirectory=/opt/aegiscms
ExecStart=/opt/aegiscms/aegis-server run
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now aegiscms
```

`/opt/aegiscms` is the extracted folder, and the `aegis` user must be able to write to it.

## 3. Windows Binary

1. Download `aegiscms-<version>-x86_64-pc-windows-msvc.zip` from [Releases](https://github.com/takumi-mr/AegisCMS/releases/latest).
2. Extract it, open a terminal **in the extracted folder**, and run:

```powershell
.\aegis-server.exe run
```

Open `http://localhost:8080/`. Admin Studio is built into the executable, and is at `/admin/`. As on Linux, it listens on `127.0.0.1` only by default.

## 4. Create an Administrator

A new site has no accounts. Create the first administrator on the machine that runs AegisCMS:

```bash
# Docker (you are asked for a password)
docker exec -it aegiscms aegis-server user add admin --role admin

# Binary (run in the extracted folder)
./aegis-server user add admin --role admin
```

For scripts, pass the password in the `AEGIS_USER_PASSWORD` environment variable instead. Then log in at `/admin/`.

Other user commands (`user list`, `user passwd`, `user disable`, and more) are in [Authentication](authentication.md).

## Next Steps

- [Publish the site on the internet with HTTPS](publish.md)
- [Back up your data](backup-restore.md) before you start adding content
- [Configuration reference](configuration.md) for every setting
