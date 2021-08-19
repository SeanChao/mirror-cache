# MirrorCache

[![Test](https://github.com/SeanChao/mirror-cache/actions/workflows/test.yml/badge.svg)](https://github.com/SeanChao/mirror-cache/actions/workflows/test.yml)

A smart reverse proxy for mirror sites with different cache policies.

## Getting Started

### Configuration

[config.yml](config.yml) is an example configuration file. Equivalent `config.{json|toml|ini}` files are also supported.
See [config-rs](https://github.com/mehcode/config-rs) for supported formats.

Exporting environment variables can override settings in the config file. All environment variables should be prefixed with `APP_`, Eg: `export APP_PORT=2333` overrides the `port` field to `2333`.

### Run

First, start a [Redis](https://redis.io/) instance and update the connection string in config (`redis.url`).
For quick start, you may use this command to start a redis server in Docker: `make redis`.

Run the app:

```sh
cargo run
```

Try it out:

```sh
pip install -i http://localhost:9000 django
```

## More

We currently support [PyPI](https://pypi.org/) and [Anaconda](https://anaconda.com).

See [docs](docs/README.md) for detailed documentation.
