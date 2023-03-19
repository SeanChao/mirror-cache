<h1 align="center">MirrorCache</h1>

<div align="center">
    <img src="https://img.shields.io/github/license/SeanChao/mirror-cache?color=blue&style=flat-square" alt="license"/>
  <a href="https://github.com/SeanChao/mirror-cache/actions/workflows/ci.yml">
    <img src="https://img.shields.io/github/workflow/status/seanchao/mirror-cache/Test?label=Test&logo=github&style=flat-square" alt="GitHub Actions CI testing status"/>
  </a>
  <a href="https://github.com/SeanChao/mirror-cache/releases/latest">
    <img src="https://img.shields.io/github/v/release/seanchao/mirror-cache?sort=semver&style=flat-square" alt="GitHub releases latest version"/>
  </a>
  <a href="https://github.com/SeanChao/mirror-cache/releases">
    <img src="https://img.shields.io/github/downloads/seanchao/mirror-cache/total?label=downloads&style=flat-square" alt="Github releases downloads"/>
  </a>
  <a href="https://hub.docker.com/r/seanchao/mirror-cache">
    <img src="https://img.shields.io/docker/pulls/seanchao/mirror-cache?style=flat-square" alt="docker pulls"/>
  </a>
  <a href="https://crates.io/crates/mirror-cache">
    <img src="https://img.shields.io/crates/d/mirror-cache?label=crates.io&style=flat-square" alt="crate.io downloads"/>
  </a>
</div>

A reverse proxy supporting multiple cache policies, customized rules, configuration hot reloading. For mirror sites and also personal users!

[docs](docs/README.md)

## Features

- Cache your dependencies in limited space with LRU/TTL cache policies
- Reload updated config without restarting the program
- Support customized rules
- Expose metrics like cache hit rate per rule, task count and more
- Support multiple storage backend (memory, local filesystem and S3)

## Quick start

1. Download the [latest release](https://github.com/SeanChao/mirror-cache/releases/latest)  
    or install with cargo:

        cargo install mirror-cache

    or pull from docker hub:

        docker pull seanchao/mirror-cache

2. Prepare a [configuration file](config.yml)

3. Try it out! e.g

    ```sh
    pip install -i http://localhost:9000 requests

    conda install -c http://localhost:9000 requests
    conda config --set custom_channels.pytorch http://localhost:9000/anaconda/cloud/ && conda install -c pytorch -y --download-only -v torchtext
    ```
