<h1 align="center">MirrorCache</h1>

<div align="center">
  <a href="https://github.com/SeanChao/mirror-cache/actions/workflows/ci.yml">
    <img src="https://github.com/SeanChao/mirror-cache/actions/workflows/ci.yml/badge.svg" alt="GitHub Actions CI status"></img>
  </a>
</div>

A reverse proxy supporting multiple cache policies, customized rules, configuration hot reloading. For mirror sites and also personal users!

[docs](docs/README.md) | [demo server](https://mirror.seanchao.xyz)

## Features

- Cache your dependencies on limited disk space with LRU/TTL cache policies
- Reload updated config without restarting the program
- Support customized rules

## Quick start

First, start a [Redis](https://redis.io/) instance and update the connection string in config (`redis.url`).
For quick start, you may use this command to start a redis server in Docker: `make redis`.

Run the app:

```sh
cargo run
```

Try it out:

```sh
pip install -i http://localhost:9000 requests

conda install -c http://localhost:9000 requests
conda config --set custom_channels.pytorch http://localhost:9000/anaconda/cloud/ && conda install -c pytorch -y --download-only -v torchtext

# Ubuntu
# In /etc/apt/sources.list: change links like http://xxx.ubuntu.com/ubuntu into http://localhost:9000/ubuntu
apt-get update
```
